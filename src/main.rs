//! Entry point: loads the config, installs the hooks, and pumps messages.
//! Win32-facing modules live in the binary; the OS-independent core
//! (keymap/config) is the `winremap` library crate so it stays testable on
//! headless CI (project brief §9).

mod hook;
mod sender;
mod window;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, bail};
use windows::Win32::UI::Accessibility::UnhookWinEvent;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, TranslateMessage,
};
use winremap::config;

fn main() -> anyhow::Result<()> {
    let config_path = parse_args()?;

    let table = config::load(&config_path)
        .with_context(|| format!("failed to load {}", config_path.display()))?;
    println!(
        "winremap {}: {} keymap(s) loaded from {}",
        env!("CARGO_PKG_VERSION"),
        table.keymaps.len(),
        config_path.display()
    );
    hook::REMAP_TABLE.store(Some(Arc::new(table)));

    sender::init_scan_codes();
    // Seed the cache before hooking so the first keystrokes resolve against
    // the correct application instead of an empty name.
    window::refresh_foreground_cache();
    let event_hook = window::install_foreground_watch().context("failed to watch foreground")?;
    let keyboard_hook = hook::install().context("failed to install keyboard hook")?;
    println!("remapping active. Press Ctrl+C to quit.");

    // Both hooks are serviced through this loop; it only exits on WM_QUIT
    // (none in v0.1 — the process ends via Ctrl+C or the Phase 3 tray).
    let mut msg = MSG::default();
    // SAFETY: msg is a live local; a null HWND means "all messages of this
    // thread", which is what hook dispatch requires.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        // SAFETY: msg was filled in by the successful GetMessageW above.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    hook::uninstall(keyboard_hook);
    // SAFETY: handle returned by install_foreground_watch, unhooked once.
    let _ = unsafe { UnhookWinEvent(event_hook) };
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
