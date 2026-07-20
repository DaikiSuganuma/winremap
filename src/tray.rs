//! Task tray UI: enable/disable toggle, config reload, open config, quit.
//!
//! Uses the `tray-icon` crate so this module stays free of `unsafe`
//! (AGENTS.md invariant 3, ADR 0007). Menu events arrive on this thread's
//! message loop and are drained by `pump_events` after each message, so no
//! extra thread or locking is involved.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::hook;
use crate::i18n;
use winremap::config;

pub struct Tray {
    icon: TrayIcon,
    enabled_item: CheckMenuItem,
    reload_item: MenuItem,
    open_item: MenuItem,
    log_item: MenuItem,
    quit_item: MenuItem,
    config_path: PathBuf,
    /// Remembered so re-enabling can restore the "N keymap(s)" tooltip.
    keymap_count: Cell<usize>,
    /// `--macro-delay` beats the config's value even across reloads
    /// (ADR 0019).
    macro_delay_override: Option<u32>,
}

pub fn init(
    config_path: PathBuf,
    keymap_count: usize,
    macro_delay_override: Option<u32>,
) -> anyhow::Result<Tray> {
    let texts = i18n::t();
    // Disabled on purpose: a caption, not a command. It also makes the menu
    // self-identifying when several tray icons look alike.
    let title_item = MenuItem::new(
        format!("{} v{}", texts.app_name, env!("CARGO_PKG_VERSION")),
        false,
        None,
    );
    let enabled_item = CheckMenuItem::new(texts.menu_enabled, true, true, None);
    let reload_item = MenuItem::new(texts.menu_reload, true, None);
    let open_item = MenuItem::new(texts.menu_open, true, None);
    let log_item = MenuItem::new(texts.menu_log, true, None);
    let quit_item = MenuItem::new(texts.menu_quit, true, None);

    let menu = Menu::new();
    menu.append_items(&[
        &title_item,
        &PredefinedMenuItem::separator(),
        &enabled_item,
        &PredefinedMenuItem::separator(),
        &reload_item,
        &open_item,
        &log_item,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ])
    .context("failed to build tray menu")?;

    let icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(i18n::tooltip_status(keymap_count))
        .with_icon(build_icon(true))
        .build()
        .context("failed to create tray icon")?;

    Ok(Tray {
        icon,
        enabled_item,
        reload_item,
        open_item,
        log_item,
        quit_item,
        config_path,
        keymap_count: Cell::new(keymap_count),
        macro_delay_override,
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
                i18n::tooltip_status(self.keymap_count.get())
            } else {
                i18n::t().tooltip_disabled.to_string()
            };
            let _ = self.icon.set_tooltip(Some(tooltip));
        } else if id == self.reload_item.id() {
            self.reload();
        } else if id == self.open_item.id() {
            open_in_default_editor(&self.config_path);
        } else if id == self.log_item.id() {
            crate::log_window::open();
        } else if id == self.quit_item.id() {
            hook::post_quit();
        }
    }

    fn reload(&self) {
        match config::load(&self.config_path) {
            Ok(table) => {
                let count = table.keymaps.len();
                crate::sender::set_macro_delay(
                    self.macro_delay_override.unwrap_or(table.macro_delay_ms),
                );
                // Atomic swap: in-flight key events keep the old table, the
                // next event sees the new one — no gap (ADR 0003).
                hook::REMAP_TABLE.store(Some(Arc::new(table)));
                // IME indicator touch point: pick up the reloaded
                // [ime_indicator] section (ADR 0020).
                crate::ime_indicator::sync_with_config();
                self.keymap_count.set(count);
                let _ = self.icon.set_tooltip(Some(i18n::tooltip_status(count)));
                crate::log_window::emit(&i18n::reload_ok(count));
                if hook::debug_enabled() {
                    crate::log_window::emit(&i18n::debug_config_loaded(&self.config_path, count));
                }
            }
            Err(e) => {
                // Keep the previous table so remapping never stops on a bad
                // edit (config-spec §4). The user just asked for this reload,
                // so a dialog (when there is no console) is expected rather
                // than intrusive — silence would look like success.
                crate::notify::error(&i18n::reload_failed(&e.to_string()));
                let _ = self.icon.set_tooltip(Some(i18n::t().tooltip_reload_failed));
            }
        }
    }
}

fn open_in_default_editor(path: &Path) {
    // `start` defers to the user's .toml file association; the empty string
    // fills start's window-title slot so the path is not mistaken for it.
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", &path.to_string_lossy()])
        .spawn();
}

/// Loads the owner-designed icon (assets/kbd*.ico, gray when disabled) from
/// the exe's embedded resources — build.rs compiles them in (ADR 0010), so
/// the binary stays a self-contained single file. `None` lets the shell pick
/// the best size from the multi-size .ico for the current DPI.
fn build_icon(enabled: bool) -> Icon {
    let ordinal = if enabled { 1 } else { 2 };
    Icon::from_resource(ordinal, None).expect("icon resources are embedded by build.rs")
}
