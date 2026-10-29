//! Entry point: loads the config, installs the hooks, and pumps messages.
//! Win32-facing modules live in the binary; the OS-independent core
//! (keymap/config) is the `winremap` library crate so it stays testable on
//! headless CI (project brief §9). This file is `unsafe`-free — Win32 calls
//! are wrapped by hook/ / window.rs / notify.rs (AGENTS.md invariant 3,
//! ADR 0009, ADR 0031).
//!
//! `run` is the startup sequence in order. The order is the point — most
//! steps exist because of what the step before them makes possible or what
//! the step after them needs — so each stage below says why it sits where it
//! does, and the stages are functions only so that the sequence reads as one
//! page (v1.1 plan E1).

// A resident tray app must not flash a console window when launched from
// Explorer, the Start menu, or the autostart entry. Terminal users still get
// output because notify::attach_parent_console hooks up to their console
// (ADR 0029), which run() lets go of again before going resident (ADR 0062).
#![windows_subsystem = "windows"]

mod cli;
mod clock;
mod cursor;
mod gui;
mod hook;
mod i18n;
mod ime_indicator;
mod keyname;
mod layout;
mod macro_record;
mod notify;
mod package;
mod sender;
mod theme;
mod tray;
mod window;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
use cli::CliArgs;
use winremap::config;
use winremap::config::last_used::LastUsed;

fn main() {
    // Before any output, so even an early failure can reach the terminal.
    notify::attach_parent_console();
    // Before anything can fail: the cursor tint is session-wide state, so
    // the paths that still run code on the way down have to put it back
    // (ADR 0067).
    cursor::install_crash_restore();
    if let Err(e) = run() {
        // `{:#}` keeps anyhow's context chain, which is what makes a config
        // error actionable ("failed to load ...: line 12: ...").
        notify::error(&format!("{e:#}"));
        // A `--debug` console belongs to this process, so it would close with
        // it — taking the message just printed with it (ADR 0068).
        notify::wait_for_debug_console();
        std::process::exit(1);
    }
    // Same for a clean exit: the shutdown transcript is the point of the flag.
    notify::wait_for_debug_console();
}

fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Language must be known before any user-facing output, including the
    // help text parse_args may print.
    i18n::init(cli::extract_lang(&args)?);
    let cli = cli::parse_args(&args)?;
    // `--debug` streams to a console of WinRemap's own, opened here so that
    // the very first line of startup lands in it — the shell's console is
    // shared with a prompt that repaints over the log, and the log window
    // cannot be opened until startup is over (ADR 0068). A redirected run
    // keeps its redirect and gets no window.
    if cli.debug {
        notify::open_debug_console();
    }
    let chosen = choose_config_file(&cli);
    hook::set_debug(cli.debug);
    #[cfg(feature = "test-inject")]
    hook::set_accept_injected(cli.accept_injected);
    // Remembered so closing the log window restores this instead of
    // silencing a `--debug` session that was running before it opened.
    gui::log::set_cli_debug(cli.debug);
    gui::set_config_path(chosen.path.clone());
    // The settings window writes this when the address bar switches files;
    // startup only reads it (ADR 0077).
    gui::set_config_memory(chosen.memory.clone());
    // The status bar shows this timestamp for the whole run.
    gui::mark_started();

    let instance = hook::acquire_single_instance().context("failed to create instance mutex")?;
    let Some(_instance) = instance else {
        bail!("{}", i18n::t().already_running);
    };

    open_session(&chosen);

    // After the session banner so first-run output is part of the transcript,
    // and before the load, which is what needs the file to be there.
    ensure_config(&chosen.path, cli.config_is_default)?;

    let keymap_count = load_config(&chosen.path, &cli)?;

    // The copy every later restore is made from, taken while nothing of ours
    // has replaced anything (ADR 0073 decision 1). Has to be after the
    // single-instance check above: a second copy of WinRemap clears the tint
    // on its way through here, and the first copy's is a tint that is
    // legitimately on.
    cursor::capture_pristine();
    // Unconditional, and before anything can tint again: a cursor left over
    // from a run that was killed is the one thing this feature cannot clear
    // by itself, and startup is when nothing can legitimately be tinted yet
    // (ADR 0067 decision 5). Costs one call when the feature is off.
    cursor::restore();

    let resident = go_resident(keymap_count, &cli)?;

    hook::run_message_loop(|| {
        resident.tray.pump_events();
        // The settings window's reload button lands here rather than on the
        // GUI thread: the tray icon belongs to this one.
        if gui::take_reload_request() {
            resident.tray.reload_now();
        }
        // Debug key events are queued by the hook (no I/O there) and
        // formatted here on the message loop (ADR 0016).
        hook::drain_debug_log();
        // Recording events are queued by the hook too: publishing a
        // finished macro allocates, so it happens here (ADR 0044).
        hook::drain_record_events();
    });

    hook::uninstall(resident.keyboard_hook);
    window::uninstall_foreground_watch(resident.event_hook);
    ime_indicator::stop();
    macro_record::stop();
    // Closes the session the startup banner opened, so a terminal transcript
    // says how long WinRemap was actually up.
    gui::log::emit(&i18n::session_ended(&clock::local_now()));
    Ok(())
}

/// Which config file this run opens, and why.
struct ChosenConfig {
    /// Absolute, and resolved to the path every process on the machine
    /// agrees about (ADR 0061).
    path: PathBuf,
    /// Where the choice is remembered between runs (ADR 0077), or `None`
    /// when there is nowhere to keep it.
    memory: Option<PathBuf>,
    /// What the memory said, so the session can say why this file was
    /// picked — or why the remembered one was not.
    recalled: LastUsed,
}

/// Stage 1: decide the config file. `--config` is an instruction about
/// *this* run and outranks a choice made in an earlier one (ADR 0077).
fn choose_config_file(cli: &CliArgs) -> ChosenConfig {
    // Where the choice of config file is remembered between runs (ADR 0077),
    // and what it says. Only consulted when this run named no file of its
    // own.
    let memory = cli::memory_path().map(package::resolve_config_path);
    let recalled = match (cli.config_is_default, memory.as_deref()) {
        (true, Some(memory)) => config::last_used::recall(memory),
        _ => LastUsed::Nothing,
    };
    let chosen = match &recalled {
        LastUsed::At(path) => path.clone(),
        // Nothing remembered, or what was remembered is gone: the default
        // path, which is where every run before 1.0.0 started.
        _ => cli.config_path.clone(),
    };
    // Absolute from the start: the settings window's address bar shows the
    // parent folder and lists its .toml files for switching (ADR 0050), and
    // a relative `--config x.toml` has no parent to show or read.
    let path = std::path::absolute(&chosen).unwrap_or(chosen);
    // Once, before anything reads or shows it: inside an MSIX package the
    // %APPDATA% path this resolves to is private to the package, and the
    // settings window hands paths to Explorer and to a text editor — neither
    // of which can see it (ADR 0061). No-op for an unpackaged run.
    let path = package::resolve_config_path(path);
    ChosenConfig {
        path,
        memory,
        recalled,
    }
}

/// Stage 2: the session's first lines, before anything that can fail from
/// here on.
fn open_session(chosen: &ChosenConfig) {
    // The log window is usually opened long after launch, so it is seeded with
    // this rather than starting mid-session with no idea when "now" began.
    let started = i18n::session_started(&clock::local_now());
    gui::log::set_session_start(&started);
    // Everything from here goes through the log, which is what decides where
    // it can be read: the console under `--debug`, the window while it is
    // open (ADR 0058). Nothing calls `notify::console_line` directly any
    // more — that is how the terminal and the window drifted apart.
    gui::log::emit(&started);
    if hook::accept_injected() {
        // Loud on purpose: this build converts other software's injected
        // input, so it must never be mistaken for a normal one (ADR 0053).
        gui::log::emit(i18n::test_build_notice());
    }

    // Why this run opened a file nobody named on the command line — or why
    // it did not (ADR 0077). Said before the load, so a config error below
    // is read next to the reason that file was picked.
    match &chosen.recalled {
        LastUsed::At(_) => gui::log::emit(&i18n::config_from_last_time(&chosen.path)),
        LastUsed::Gone(path) => gui::log::emit(&i18n::config_from_last_time_gone(path)),
        LastUsed::Nothing => {}
    }
}

/// Stage 3: compile the config and hand it to the hook. Returns how many
/// keymaps it holds, which the tray's tooltip reports.
fn load_config(config_path: &Path, cli: &CliArgs) -> anyhow::Result<usize> {
    // Which key `;` is depends on the keyboard, so the config cannot be
    // compiled until Windows has been asked (ADR 0063). On the main thread on
    // purpose: the answer is per-thread.
    let keyboard = layout::refresh();

    // A startup config error aborts: better to not run at all than to sit in
    // the tray silently doing nothing the user asked for (config-spec §4).
    let table = config::load(config_path, &keyboard)
        .with_context(|| format!("failed to load {}", config_path.display()))?;
    let keymap_count = table.keymaps.len();
    gui::log::emit(&i18n::startup_loaded(keymap_count, config_path));
    // Precedence: --macro-delay > config's macro_delay_ms > 0 (ADR 0019).
    sender::set_macro_delay(cli.macro_delay_ms.unwrap_or(table.macro_delay_ms));
    hook::REMAP_TABLE.store(Some(Arc::new(table)));
    gui::mark_config_loaded();
    Ok(keymap_count)
}

/// What stage 4 installed, and what shutdown has to take down again.
struct Resident {
    event_hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    keyboard_hook: windows::Win32::UI::WindowsAndMessaging::HHOOK,
    tray: tray::Tray,
}

/// Stage 4: hooks, tray and feature threads, in the order that lets each
/// find what it needs already there — then the console is let go.
fn go_resident(keymap_count: usize, cli: &CliArgs) -> anyhow::Result<Resident> {
    sender::init_scan_codes();
    // Seed the cache before hooking so the first keystrokes resolve against
    // the correct application instead of an empty name.
    window::refresh_foreground_cache();
    let event_hook = window::install_foreground_watch().context("failed to watch foreground")?;
    let keyboard_hook = hook::install().context("failed to install keyboard hook")?;
    let tray = tray::init(keymap_count, cli.macro_delay_ms).context("failed to set up tray")?;
    // IME indicator touch point: starts its thread only when the config
    // enables the feature (ADR 0020).
    ime_indicator::sync_with_config();
    // Macro recording touch point: same shape — no thread unless [macro]
    // names the recording keys (ADR 0043/0044).
    macro_record::sync_with_config();
    gui::log::emit(i18n::t().remapping_active);

    // Startup is over, so the terminal that launched us has had everything it
    // is going to get: let its console go, or closing that terminal would take
    // a working WinRemap with it (ADR 0062). Late on purpose — every failure
    // above this line still reaches the terminal it was reported from.
    // `--debug` is the exception: it has a console of its own by now, and
    // that one stays for the whole run (ADR 0068).
    if !cli.debug {
        notify::detach_console();
    }
    Ok(Resident {
        event_hook,
        keyboard_hook,
        tray,
    })
}

/// Makes sure `path` exists before the config is loaded.
///
/// A first run has no config, and WinRemap used to refuse to start — which
/// left the app depending on the installer having seeded the file. That holds
/// for the Inno installer but not for the portable exe, nor for an MSIX
/// package, which has no install-time script at all (ADR 0059). So the
/// default path is created on demand instead.
///
/// A path the user named with `--config` is still an error when missing: a
/// typo should say so rather than quietly produce an empty remapping.
fn ensure_config(path: &Path, is_default: bool) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if !is_default {
        bail!("{}", i18n::no_config_file(path));
    }
    config::create_default(path).with_context(|| format!("failed to create {}", path.display()))?;
    gui::log::emit(&i18n::created_default_config(path));
    Ok(())
}
