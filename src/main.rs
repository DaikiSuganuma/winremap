//! Entry point: loads the config, installs the hooks, and pumps messages.
//! Win32-facing modules live in the binary; the OS-independent core
//! (keymap/config) is the `winremap` library crate so it stays testable on
//! headless CI (project brief §9). This file is `unsafe`-free — Win32 calls
//! are wrapped by hook.rs / window.rs / notify.rs (AGENTS.md invariant 3,
//! ADR 0009, ADR 0031).

// A resident tray app must not flash a console window when launched from
// Explorer, the Start menu, or the autostart entry. Terminal users still get
// output because notify::attach_parent_console hooks up to their console
// (ADR 0029).
#![windows_subsystem = "windows"]

mod hook;
mod i18n;
mod ime_indicator;
mod notify;
mod sender;
mod tray;
mod window;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, bail};
use winremap::config;

fn main() {
    // Before any output, so even an early failure can reach the terminal.
    notify::attach_parent_console();
    if let Err(e) = run() {
        // `{:#}` keeps anyhow's context chain, which is what makes a config
        // error actionable ("failed to load ...: line 12: ...").
        notify::error(&format!("{e:#}"));
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Language must be known before any user-facing output, including the
    // help text parse_args may print.
    i18n::init(extract_lang(&args)?);
    let cli = parse_args(&args)?;
    let config_path = cli.config_path;
    hook::set_debug(cli.debug);

    let instance = hook::acquire_single_instance().context("failed to create instance mutex")?;
    let Some(_instance) = instance else {
        bail!("{}", i18n::t().already_running);
    };

    // A startup config error aborts: better to not run at all than to sit in
    // the tray silently doing nothing the user asked for (config-spec §4).
    let table = config::load(&config_path)
        .with_context(|| format!("failed to load {}", config_path.display()))?;
    let keymap_count = table.keymaps.len();
    notify::console_line(&i18n::startup_loaded(keymap_count, &config_path));
    if hook::debug_enabled() {
        notify::console_line(&i18n::debug_config_loaded(&config_path, keymap_count));
    }
    // Precedence: --macro-delay > config's macro_delay_ms > 0 (ADR 0019).
    sender::set_macro_delay(cli.macro_delay_ms.unwrap_or(table.macro_delay_ms));
    hook::REMAP_TABLE.store(Some(Arc::new(table)));

    sender::init_scan_codes();
    // Seed the cache before hooking so the first keystrokes resolve against
    // the correct application instead of an empty name.
    window::refresh_foreground_cache();
    let event_hook = window::install_foreground_watch().context("failed to watch foreground")?;
    let keyboard_hook = hook::install().context("failed to install keyboard hook")?;
    let tray = tray::init(config_path, keymap_count, cli.macro_delay_ms)
        .context("failed to set up tray")?;
    // IME indicator touch point: starts its thread only when the config
    // enables the feature (ADR 0020).
    ime_indicator::sync_with_config();
    notify::console_line(i18n::t().remapping_active);

    hook::run_message_loop(|| {
        tray.pump_events();
        // Debug key events are queued by the hook (no I/O there) and
        // formatted here on the message loop (ADR 0016).
        hook::drain_debug_log();
    });

    hook::uninstall(keyboard_hook);
    window::uninstall_foreground_watch(event_hook);
    ime_indicator::stop();
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
    debug: bool,
    /// `None` when the flag was absent, so the config file's value applies.
    macro_delay_ms: Option<u32>,
}

fn parse_args(args: &[String]) -> anyhow::Result<CliArgs> {
    let mut config: Option<PathBuf> = None;
    let mut debug = false;
    let mut macro_delay_ms: Option<u32> = None;
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
    let config_path = match config {
        Some(path) => path,
        None => default_config_path()?,
    };
    Ok(CliArgs {
        config_path,
        debug,
        macro_delay_ms,
    })
}

fn default_config_path() -> anyhow::Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA")
        .context("APPDATA is not set; pass --config <path> explicitly")?;
    let path = PathBuf::from(appdata).join("winremap").join("config.toml");
    if !path.exists() {
        bail!("{}", i18n::no_config_file(&path));
    }
    Ok(path)
}
