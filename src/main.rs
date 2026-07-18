//! Entry point: loads the config, installs the hooks, and pumps messages.
//! Win32-facing modules live in the binary; the OS-independent core
//! (keymap/config) is the `winremap` library crate so it stays testable on
//! headless CI (project brief §9). This file is `unsafe`-free — Win32 calls
//! are wrapped by hook.rs / window.rs (AGENTS.md invariant 3, ADR 0009).

mod hook;
mod sender;
mod tray;
mod window;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, bail};
use winremap::config;

fn main() -> anyhow::Result<()> {
    let config_path = parse_args()?;

    let instance = hook::acquire_single_instance().context("failed to create instance mutex")?;
    let Some(_instance) = instance else {
        bail!("winremap is already running (check the task tray)");
    };

    // A startup config error aborts: better to not run at all than to sit in
    // the tray silently doing nothing the user asked for (config-spec §4).
    let table = config::load(&config_path)
        .with_context(|| format!("failed to load {}", config_path.display()))?;
    let keymap_count = table.keymaps.len();
    println!(
        "winremap {}: {} keymap(s) loaded from {}",
        env!("CARGO_PKG_VERSION"),
        keymap_count,
        config_path.display()
    );
    hook::REMAP_TABLE.store(Some(Arc::new(table)));

    sender::init_scan_codes();
    // Seed the cache before hooking so the first keystrokes resolve against
    // the correct application instead of an empty name.
    window::refresh_foreground_cache();
    let event_hook = window::install_foreground_watch().context("failed to watch foreground")?;
    let keyboard_hook = hook::install().context("failed to install keyboard hook")?;
    let tray = tray::init(config_path, keymap_count).context("failed to set up tray")?;
    println!("remapping active. Use the tray icon to reload or quit.");

    hook::run_message_loop(|| tray.pump_events());

    hook::uninstall(keyboard_hook);
    window::uninstall_foreground_watch(event_hook);
    Ok(())
}

fn parse_args() -> anyhow::Result<PathBuf> {
    let mut config: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" | "-c" => {
                let value = args.next().context("--config requires a path")?;
                config = Some(PathBuf::from(value));
            }
            "--version" | "-V" => {
                println!("winremap {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => bail!("unknown argument `{other}` (try --help)"),
        }
    }
    match config {
        Some(path) => Ok(path),
        None => default_config_path(),
    }
}

fn default_config_path() -> anyhow::Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA")
        .context("APPDATA is not set; pass --config <path> explicitly")?;
    let path = PathBuf::from(appdata).join("winremap").join("config.toml");
    if !path.exists() {
        bail!(
            "no config file at {}.\nCreate it (see examples/minimal.toml) or pass --config <path>.",
            path.display()
        );
    }
    Ok(path)
}

fn print_help() {
    println!(
        "winremap {} — per-application key remapper for Windows

USAGE:
    winremap [OPTIONS]

OPTIONS:
    -c, --config <PATH>    Config file (default: %APPDATA%\\winremap\\config.toml)
    -V, --version          Print version
    -h, --help             Print this help",
        env!("CARGO_PKG_VERSION")
    );
}
