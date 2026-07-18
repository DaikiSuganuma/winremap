//! Task tray UI: enable/disable toggle, config reload, open config, quit.
//!
//! Uses the `tray-icon` crate so this module stays free of `unsafe`
//! (AGENTS.md invariant 3, ADR 0007). Menu events arrive on this thread's
//! message loop and are drained by `pump_events` after each message, so no
//! extra thread or locking is involved.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::hook;
use winremap::config;

pub struct Tray {
    icon: TrayIcon,
    enabled_item: CheckMenuItem,
    reload_item: MenuItem,
    open_item: MenuItem,
    quit_item: MenuItem,
    config_path: PathBuf,
}

pub fn init(config_path: PathBuf, keymap_count: usize) -> anyhow::Result<Tray> {
    let enabled_item = CheckMenuItem::new("Enabled", true, true, None);
    let reload_item = MenuItem::new("Reload config", true, None);
    let open_item = MenuItem::new("Open config file", true, None);
    let quit_item = MenuItem::new("Quit", true, None);

    let menu = Menu::new();
    menu.append_items(&[
        &enabled_item,
        &PredefinedMenuItem::separator(),
        &reload_item,
        &open_item,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ])
    .context("failed to build tray menu")?;

    let icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(status_tooltip(keymap_count))
        .with_icon(build_icon(true))
        .build()
        .context("failed to create tray icon")?;

    Ok(Tray {
        icon,
        enabled_item,
        reload_item,
        open_item,
        quit_item,
        config_path,
    })
}

impl Tray {
    /// Drains pending menu clicks. Called from the message-loop callback —
    /// events sit in a channel until then, which is fine because the menu
    /// itself is only interactable while the loop is pumping.
    pub fn pump_events(&self) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            self.handle(&event);
        }
    }

    fn handle(&self, event: &MenuEvent) {
        let id = event.id();
        if id == self.enabled_item.id() {
            // CheckMenuItem toggles its own checked state on click; the item
            // is the source of truth and the hook flag follows it.
            let enabled = self.enabled_item.is_checked();
            hook::set_enabled(enabled);
            let _ = self.icon.set_icon(Some(build_icon(enabled)));
            let tooltip = if enabled {
                "winremap"
            } else {
                "winremap (disabled)"
            };
            let _ = self.icon.set_tooltip(Some(tooltip));
        } else if id == self.reload_item.id() {
            self.reload();
        } else if id == self.open_item.id() {
            open_in_default_editor(&self.config_path);
        } else if id == self.quit_item.id() {
            hook::post_quit();
        }
    }

    fn reload(&self) {
        match config::load(&self.config_path) {
            Ok(table) => {
                let count = table.keymaps.len();
                // Atomic swap: in-flight key events keep the old table, the
                // next event sees the new one — no gap (ADR 0003).
                hook::REMAP_TABLE.store(Some(Arc::new(table)));
                let _ = self.icon.set_tooltip(Some(status_tooltip(count)));
                println!("config reloaded: {count} keymap(s)");
            }
            Err(e) => {
                // Keep the previous table so remapping never stops on a bad
                // edit (config-spec §4); surface the error where we can.
                eprintln!("config reload failed, keeping previous config:\n{e}");
                let _ = self
                    .icon
                    .set_tooltip(Some("winremap — config reload FAILED (see console)"));
            }
        }
    }
}

fn status_tooltip(keymap_count: usize) -> String {
    format!("winremap — {keymap_count} keymap(s)")
}

fn open_in_default_editor(path: &Path) {
    // `start` defers to the user's .toml file association; the empty string
    // fills start's window-title slot so the path is not mistaken for it.
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", &path.to_string_lossy()])
        .spawn();
}

/// 32x32 RGBA icon drawn in code: a rounded square with two rows of keycaps.
/// Generated rather than shipped as an asset to keep the binary
/// self-contained; gray when disabled so the state is visible at a glance.
fn build_icon(enabled: bool) -> Icon {
    const SIZE: usize = 32;
    const CORNER: usize = 5;
    let (r, g, b) = if enabled {
        (0x00, 0x78, 0xD7) // Windows accent blue
    } else {
        (0x6E, 0x6E, 0x6E)
    };

    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            // Clip the four corners for a rounded-square silhouette.
            let dx = CORNER.saturating_sub(x.min(SIZE - 1 - x));
            let dy = CORNER.saturating_sub(y.min(SIZE - 1 - y));
            if dx + dy > CORNER {
                continue;
            }
            let i = (y * SIZE + x) * 4;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 0xFF;
        }
    }

    // Two rows of three white "keycaps" suggest a keyboard.
    for y0 in [8usize, 18] {
        for col in 0..3usize {
            let x0 = 6 + col * 8;
            for y in y0..y0 + 6 {
                for x in x0..x0 + 6 {
                    let i = (y * SIZE + x) * 4;
                    rgba[i] = 0xFF;
                    rgba[i + 1] = 0xFF;
                    rgba[i + 2] = 0xFF;
                    rgba[i + 3] = 0xFF;
                }
            }
        }
    }

    Icon::from_rgba(rgba, SIZE as u32, SIZE as u32)
        .expect("icon buffer dimensions are statically correct")
}
