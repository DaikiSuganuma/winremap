//! Entry point: loads the config, installs the hooks, and pumps messages.
//! Win32-facing modules live in the binary; the OS-independent core
//! (keymap/config) is the `winremap` library crate so it stays testable on
//! headless CI (project brief §9). This file is `unsafe`-free — Win32 calls
//! are wrapped by hook.rs / window.rs (AGENTS.md invariant 3, ADR 0009).

mod hook;
mod i18n;
mod sender;
mod tray;
mod window;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, bail};
use winremap::config;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Language must be known before any user-facing output, including the
    // help text parse_args may print.
    i18n::init(extract_lang(&args)?);
    let cli = parse_args(&args)?;
    let config_path = cli.config_path;
    hook::set_debug(cli.debug);
    sender::set_macro_delay(cli.macro_delay_ms);

    let instance = hook::acquire_single_instance().context("failed to create instance mutex")?;
    let Some(_instance) = instance else {
        bail!("{}", i18n::t().already_running);
    };

    // A startup config error aborts: better to not run at all than to sit in
    // the tray silently doing nothing the user asked for (config-spec §4).
    let table = config::load(&config_path)
        .with_context(|| format!("failed to load {}", config_path.display()))?;
    let keymap_count = table.keymaps.len();
    println!("{}", i18n::startup_loaded(keymap_count, &config_path));
    hook::REMAP_TABLE.store(Some(Arc::new(table)));

    sender::init_scan_codes();
    // Seed the cache before hooking so the first keystrokes resolve against
    // the correct application instead of an empty name.
    window::refresh_foreground_cache();
    let event_hook = window::install_foreground_watch().context("failed to watch foreground")?;
    let keyboard_hook = hook::install().context("failed to install keyboard hook")?;
    let tray = tray::init(config_path, keymap_count).context("failed to set up tray")?;
    println!("{}", i18n::t().remapping_active);

    hook::run_message_loop(|| {
        tray.pump_events();
        // Debug key events are queued by the hook (no I/O there) and
        // formatted here on the message loop (ADR 0016).
        hook::drain_debug_log();
    });

    hook::uninstall(keyboard_hook);
    window::uninstall_foreground_watch(event_hook);
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
    macro_delay_ms: u32,
}

fn parse_args(args: &[String]) -> anyhow::Result<CliArgs> {
    let mut config: Option<PathBuf> = None;
    let mut debug = false;
    let mut macro_delay_ms = 0u32;
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
                macro_delay_ms = value
                    .parse()
                    .ok()
                    .filter(|&ms| ms <= sender::MAX_MACRO_DELAY_MS)
                    .with_context(|| {
                        format!(
                            "invalid --macro-delay `{value}` (expected 0-{})",
                            sender::MAX_MACRO_DELAY_MS
                        )
                    })?;
            }
            "--version" | "-V" => {
                println!("winremap {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!("{}", i18n::help_text());
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
