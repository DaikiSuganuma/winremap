//! Entry point: loads the config, installs the hooks, and pumps messages.
//! Win32-facing modules live in the binary; the OS-independent core
//! (keymap/config) is the `winremap` library crate so it stays testable on
//! headless CI (project brief §9). This file is `unsafe`-free — Win32 calls
//! are wrapped by hook.rs / window.rs / notify.rs (AGENTS.md invariant 3,
//! ADR 0009, ADR 0031).

// A resident tray app must not flash a console window when launched from
// Explorer, the Start menu, or the autostart entry. Terminal users still get
// output because notify::attach_parent_console hooks up to their console
// (ADR 0029), which run() lets go of again before going resident (ADR 0062).
#![windows_subsystem = "windows"]

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
use winremap::config;

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
    i18n::init(extract_lang(&args)?);
    let cli = parse_args(&args)?;
    // `--debug` streams to a console of WinRemap's own, opened here so that
    // the very first line of startup lands in it — the shell's console is
    // shared with a prompt that repaints over the log, and the log window
    // cannot be opened until startup is over (ADR 0068). A redirected run
    // keeps its redirect and gets no window.
    if cli.debug {
        notify::open_debug_console();
    }
    // Absolute from the start: the settings window's address bar shows the
    // parent folder and lists its .toml files for switching (ADR 0050), and
    // a relative `--config x.toml` has no parent to show or read.
    let config_path = std::path::absolute(&cli.config_path).unwrap_or(cli.config_path);
    // Once, before anything reads or shows it: inside an MSIX package the
    // %APPDATA% path this resolves to is private to the package, and the
    // settings window hands paths to Explorer and to a text editor — neither
    // of which can see it (ADR 0061). No-op for an unpackaged run.
    let config_path = package::resolve_config_path(config_path);
    hook::set_debug(cli.debug);
    #[cfg(feature = "test-inject")]
    hook::set_accept_injected(cli.accept_injected);
    // Remembered so closing the log window restores this instead of
    // silencing a `--debug` session that was running before it opened.
    gui::log::set_cli_debug(cli.debug);
    gui::set_config_path(config_path.clone());
    // The status bar shows this timestamp for the whole run.
    gui::mark_started();

    let instance = hook::acquire_single_instance().context("failed to create instance mutex")?;
    let Some(_instance) = instance else {
        bail!("{}", i18n::t().already_running);
    };

    // The session's first line, before anything that can fail from here on.
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

    // After the session banner so first-run output is part of the transcript,
    // and before the load, which is what needs the file to be there.
    ensure_config(&config_path, cli.config_is_default)?;

    // Which key `;` is depends on the keyboard, so the config cannot be
    // compiled until Windows has been asked (ADR 0063). On the main thread on
    // purpose: the answer is per-thread.
    let keyboard = layout::refresh();

    // A startup config error aborts: better to not run at all than to sit in
    // the tray silently doing nothing the user asked for (config-spec §4).
    let table = config::load(&config_path, &keyboard)
        .with_context(|| format!("failed to load {}", config_path.display()))?;
    let keymap_count = table.keymaps.len();
    gui::log::emit(&i18n::startup_loaded(keymap_count, &config_path));
    // Precedence: --macro-delay > config's macro_delay_ms > 0 (ADR 0019).
    sender::set_macro_delay(cli.macro_delay_ms.unwrap_or(table.macro_delay_ms));
    hook::REMAP_TABLE.store(Some(Arc::new(table)));
    gui::mark_config_loaded();

    // Unconditional, and before anything can tint again: a cursor left over
    // from a run that was killed is the one thing this feature cannot clear
    // by itself, and startup is when nothing can legitimately be tinted yet
    // (ADR 0067 decision 5). Costs one call when the feature is off.
    cursor::restore();

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

    hook::run_message_loop(|| {
        tray.pump_events();
        // The settings window's reload button lands here rather than on the
        // GUI thread: the tray icon belongs to this one.
        if gui::take_reload_request() {
            tray.reload_now();
        }
        // Debug key events are queued by the hook (no I/O there) and
        // formatted here on the message loop (ADR 0016).
        hook::drain_debug_log();
        // Recording events are queued by the hook too: publishing a
        // finished macro allocates, so it happens here (ADR 0044).
        hook::drain_record_events();
    });

    hook::uninstall(keyboard_hook);
    window::uninstall_foreground_watch(event_hook);
    ime_indicator::stop();
    macro_record::stop();
    // Closes the session the startup banner opened, so a terminal transcript
    // says how long WinRemap was actually up.
    gui::log::emit(&i18n::session_ended(&clock::local_now()));
    Ok(())
}

/// Pre-scan for `--lang` so i18n can initialize before any other output.
fn extract_lang(args: &[String]) -> anyhow::Result<Option<i18n::Lang>> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--lang" {
            return match iter.next().map(String::as_str) {
                Some("en") => Ok(Some(i18n::Lang::En)),
                Some("ja") => Ok(Some(i18n::Lang::Ja)),
                // English on purpose: i18n is not initialized yet.
                other => bail!(
                    "invalid --lang value `{}` (expected `en` or `ja`)",
                    other.unwrap_or("")
                ),
            };
        }
    }
    Ok(None)
}

struct CliArgs {
    config_path: PathBuf,
    /// `false` when `--config` named the path. Decides whether a missing file
    /// is created or reported (see `ensure_config`).
    config_is_default: bool,
    debug: bool,
    /// `None` when the flag was absent, so the config file's value applies.
    macro_delay_ms: Option<u32>,
    /// Test-only remapping of other software's injected events (ADR 0053).
    #[cfg(feature = "test-inject")]
    accept_injected: bool,
}

fn parse_args(args: &[String]) -> anyhow::Result<CliArgs> {
    let mut config: Option<PathBuf> = None;
    let mut debug = false;
    let mut macro_delay_ms: Option<u32> = None;
    #[cfg(feature = "test-inject")]
    let mut accept_injected = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" | "-c" => {
                let value = iter.next().context("--config requires a path")?;
                config = Some(PathBuf::from(value));
            }
            // Already consumed by extract_lang; skip its value here.
            "--lang" => {
                iter.next();
            }
            "--debug" => debug = true,
            // Absent from ordinary builds, where it falls to the unknown
            // argument arm below like any other typo (ADR 0053).
            #[cfg(feature = "test-inject")]
            "--accept-injected" => accept_injected = true,
            "--macro-delay" => {
                let value = iter.next().context("--macro-delay requires milliseconds")?;
                let max = winremap::keymap::MAX_MACRO_DELAY_MS;
                macro_delay_ms = Some(value.parse().ok().filter(|&ms| ms <= max).with_context(
                    || format!("invalid --macro-delay `{value}` (expected 0-{max})"),
                )?);
            }
            // Both go through notify so a shortcut carrying the flag still
            // shows something instead of exiting silently.
            "--version" | "-V" => {
                notify::info(&format!("winremap {}", env!("CARGO_PKG_VERSION")));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                notify::info(&i18n::help_text());
                std::process::exit(0);
            }
            other => bail!("{}", i18n::unknown_argument(other)),
        }
    }
    let config_is_default = config.is_none();
    let config_path = match config {
        Some(path) => path,
        None => default_config_path()?,
    };
    Ok(CliArgs {
        config_path,
        config_is_default,
        debug,
        macro_delay_ms,
        #[cfg(feature = "test-inject")]
        accept_injected,
    })
}

/// Where a config lives when `--config` was not given. Existence is not
/// checked here: `parse_args` stays free of filesystem side effects, and
/// creating the file is the startup sequence's job (see `ensure_config`).
fn default_config_path() -> anyhow::Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA")
        .context("APPDATA is not set; pass --config <path> explicitly")?;
    Ok(PathBuf::from(appdata).join("winremap").join("config.toml"))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `--config` keeps the cases off %APPDATA%, which may hold a real config.
    fn parse(extra: &[&str]) -> anyhow::Result<CliArgs> {
        let mut args = vec!["--config".to_owned(), "x.toml".to_owned()];
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        parse_args(&args)
    }

    /// The guarantee of ADR 0053: a shipped build does not know the flag. A
    /// misplaced `#[cfg]` would silently hand it to every user instead.
    #[test]
    #[cfg(not(feature = "test-inject"))]
    fn accept_injected_is_unknown_without_the_feature() {
        let Err(err) = parse(&["--accept-injected"]) else {
            panic!("--accept-injected must not be accepted outside a test build");
        };
        assert!(err.to_string().contains("--accept-injected"));
    }

    #[test]
    #[cfg(feature = "test-inject")]
    fn accept_injected_is_off_until_asked_for() {
        assert!(!parse(&[]).unwrap().accept_injected);
        assert!(parse(&["--accept-injected"]).unwrap().accept_injected);
    }
}
