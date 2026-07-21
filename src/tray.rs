//! Task tray UI: enable/disable toggle, settings, config reload, log, quit.
//!
//! Uses the `tray-icon` crate so this module stays free of `unsafe`
//! (AGENTS.md invariant 3, ADR 0007). Menu events arrive on this thread's
//! message loop and are drained by `pump_events` after each message, so no
//! extra thread or locking is involved.

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use tray_icon::menu::{CheckMenuItem, IconMenuItem, Menu, MenuEvent, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::hook;
use crate::i18n;
use winremap::config;

pub struct Tray {
    icon: TrayIcon,
    enabled_item: CheckMenuItem,
    reload_item: IconMenuItem,
    settings_item: IconMenuItem,
    log_item: IconMenuItem,
    quit_item: IconMenuItem,
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
    let title_item = IconMenuItem::new(
        format!("{} v{}", texts.app_name, env!("CARGO_PKG_VERSION")),
        false,
        app_menu_icon(),
        None,
    );
    // No icon: the checkmark is this item's own marker, and a second glyph
    // beside it would only compete with it.
    let enabled_item = CheckMenuItem::new(texts.menu_enabled, true, true, None);
    let reload_item = IconMenuItem::new(texts.menu_reload, true, menu_icon(RELOAD_ICON), None);
    let settings_item =
        IconMenuItem::new(texts.menu_settings, true, menu_icon(SETTINGS_ICON), None);
    let log_item = IconMenuItem::new(texts.menu_log, true, menu_icon(LOG_ICON), None);
    let quit_item = IconMenuItem::new(texts.menu_quit, true, menu_icon(QUIT_ICON), None);

    let menu = Menu::new();
    menu.append_items(&[
        &title_item,
        &PredefinedMenuItem::separator(),
        &enabled_item,
        &PredefinedMenuItem::separator(),
        &settings_item,
        &reload_item,
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
        settings_item,
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
            crate::gui::log::action(&i18n::toggle_state(enabled));
        } else if id == self.reload_item.id() {
            crate::gui::log::action(i18n::t().menu_reload);
            self.reload();
        } else if id == self.settings_item.id() {
            crate::gui::open_config();
        } else if id == self.log_item.id() {
            crate::gui::open_log();
        } else if id == self.quit_item.id() {
            crate::gui::log::action(i18n::t().menu_quit);
            hook::post_quit();
        }
    }

    /// A reload asked for from somewhere other than the menu — today, the
    /// settings window's button. It runs here because the tray icon and its
    /// tooltip belong to the thread that created them.
    pub fn reload_now(&self) {
        self.reload();
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
                // A reload can change (or remove) the keys that end a
                // recording, so an in-progress one is dropped rather than
                // left with no way out (design doc §5.6).
                hook::abort_recording(i18n::t().macro_record_reason_reload);
                crate::macro_record::sync_with_config();
                self.keymap_count.set(count);
                crate::gui::mark_config_loaded();
                let _ = self.icon.set_tooltip(Some(i18n::tooltip_status(count)));
                crate::gui::log::emit(&i18n::reload_ok(count));
                if hook::debug_enabled() {
                    crate::gui::log::emit(&i18n::debug_config_loaded(&self.config_path, count));
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

/// Menu icons, rasterized from Bootstrap Icons SVGs by build.rs (ADR 0040):
/// 16x16 straight RGBA, which is the only thing a Win32 menu takes.
const MENU_ICON_SIZE: u32 = 16;
const SETTINGS_ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/menu-gear.rgba"));
const RELOAD_ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/menu-arrow-clockwise.rgba"));
const LOG_ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/menu-card-list.rgba"));
const QUIT_ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/menu-box-arrow-right.rgba"));

fn menu_icon(rgba: &[u8]) -> Option<tray_icon::menu::Icon> {
    tray_icon::menu::Icon::from_rgba(rgba.to_vec(), MENU_ICON_SIZE, MENU_ICON_SIZE).ok()
}

/// The app icon at menu size, for the caption row. Decoded from the 16 px PNG
/// because menu items take raw pixels, not an .ico; `None` just leaves the
/// row without an icon.
fn app_menu_icon() -> Option<tray_icon::menu::Icon> {
    let png = include_bytes!("../assets/png/kbd-enabled-16.png");
    let data = eframe::icon_data::from_png_bytes(png).ok()?;
    tray_icon::menu::Icon::from_rgba(data.rgba, data.width, data.height).ok()
}

/// Loads the owner-designed icon (assets/kbd*.ico, gray when disabled) from
/// the exe's embedded resources — build.rs compiles them in (ADR 0010), so
/// the binary stays a self-contained single file. `None` lets the shell pick
/// the best size from the multi-size .ico for the current DPI.
fn build_icon(enabled: bool) -> Icon {
    let ordinal = if enabled { 1 } else { 2 };
    Icon::from_resource(ordinal, None).expect("icon resources are embedded by build.rs")
}
