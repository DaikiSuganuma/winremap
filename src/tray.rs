//! Task tray UI: enable/disable toggle, settings, config reload, log, quit.
//!
//! Uses the `tray-icon` crate so this module stays free of `unsafe`
//! (AGENTS.md invariant 3, ADR 0007). Menu events arrive on this thread's
//! message loop and are drained by `pump_events` after each message, so no
//! extra thread or locking is involved.

use std::cell::Cell;
use std::sync::Arc;

use anyhow::Context;
use tray_icon::menu::{CheckMenuItem, IconMenuItem, Menu, MenuEvent, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::hook;
use crate::i18n;
use winremap::config;

pub struct Tray {
    icon: TrayIcon,
    /// The caption naming the config file in force; see [`Tray::reload`] for
    /// why it is only ever set after a load that worked (ADR 0079).
    config_item: IconMenuItem,
    enabled_item: CheckMenuItem,
    reload_item: IconMenuItem,
    settings_item: IconMenuItem,
    log_item: IconMenuItem,
    quit_item: IconMenuItem,
    /// Remembered so re-enabling can restore the "N keymap(s)" tooltip.
    keymap_count: Cell<usize>,
    /// `--macro-delay` beats the config's value even across reloads
    /// (ADR 0019).
    macro_delay_override: Option<u32>,
}

pub fn init(keymap_count: usize, macro_delay_override: Option<u32>) -> anyhow::Result<Tray> {
    let texts = i18n::t();
    // Disabled on purpose: a caption, not a command. It also makes the menu
    // self-identifying when several tray icons look alike.
    let test_build = if crate::hook::accept_injected() {
        i18n::test_build_tray_suffix()
    } else {
        ""
    };
    let title_item = IconMenuItem::new(
        format!(
            "{} v{}{test_build}",
            texts.app_name,
            env!("CARGO_PKG_VERSION")
        ),
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

    // The second caption: which of the folder's `*.toml` files the keymaps in
    // force came from (ADR 0079). Disabled like the version above it — the
    // menu is where a resident app answers "what am I running", and since v0.4
    // that answer has had two halves (ADR 0050 lets the settings window switch
    // files, and ADR 0077 makes the choice outlive the run).
    //
    // Named at startup from the load `main` has already done, so the caption
    // never says a file that failed to load.
    //
    // **Created last, though it is shown second.** `muda` hands out command
    // ids in creation order, and the UI checks drive this menu by id — 1003 is
    // "Settings" — precisely so they do not depend on the guest's display
    // language (ADR 0064). Creating this one where it appears shifted every id
    // by one, and six of the ten VM checks invoked the wrong item while
    // reporting the invoke itself as a success (2026-08-16). The menu's order
    // lives in `append_items` below, which is free to differ.
    let config_item = IconMenuItem::new(
        config_file_name(&crate::gui::active_config_path()),
        false,
        menu_icon(FILE_ICON),
        None,
    );

    let menu = Menu::new();
    menu.append_items(&[
        &title_item,
        &config_item,
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
        config_item,
        enabled_item,
        reload_item,
        settings_item,
        log_item,
        quit_item,
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
        // Read fresh each time, not held as a field: the address bar can
        // switch the active file, and a copy here would keep reloading the
        // old one (ADR 0050).
        let config_path = crate::gui::active_config_path();
        // Re-read the keyboard, not just the file: a reload is also how a
        // user who swapped keyboards gets `;` pointing at the right key
        // again, without restarting (ADR 0063). This runs on the message
        // loop's thread, the same one that read it at startup.
        let keyboard = crate::layout::refresh();
        match config::load(&config_path, &keyboard) {
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
                // Here rather than at the top of this function: the caption
                // says which file the keymaps in force came from, and after a
                // switch that failed to load those are still the old file's
                // (ADR 0050 decision 3 keeps the live table). Naming the file
                // that did not load would be the one reading of "currently
                // loaded" that is untrue.
                self.config_item.set_text(config_file_name(&config_path));
                crate::gui::mark_config_loaded();
                let _ = self.icon.set_tooltip(Some(i18n::tooltip_status(count)));
                crate::gui::log::action(&i18n::reload_ok(count));
                if hook::debug_enabled() {
                    crate::gui::log::action(&i18n::debug_config_loaded(&config_path, count));
                }
            }
            Err(e) => {
                // Keep the previous table so remapping never stops on a bad
                // edit (config-spec §4). The user just asked for this reload,
                // so a dialog (when there is no console) is expected rather
                // than intrusive — silence would look like success.
                let message = i18n::reload_failed(&e.to_string());
                // The settings window says so itself as well. `notify::error`
                // routes to the console when one is attached, and a terminal
                // behind the window is not where the answer is looked for
                // (owner feedback 2026-07-26, acceptance B-5); its status bar
                // would otherwise still read "loaded" from the last success.
                crate::gui::set_status(&message);
                crate::notify::error(&message);
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
const FILE_ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/menu-file-earmark-text.rgba"));

/// The config file's name for the caption. The folder is deliberately left
/// out: it is the settings window's address bar that answers "where", and a
/// packaged install's folder is long enough to make the menu unreadable
/// (ADR 0078 had to shorten it even in a window).
fn config_file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

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
/// the binary stays a self-contained single file.
///
/// Asks for the notification area's own size rather than letting `tray-icon`
/// pick: its `from_resource(_, None)` means "the large metric", and the shell
/// shrinking that 32 px face into a 16 px slot closed up the gaps between the
/// keys, leaving a blue block (ADR 0080). `from_resource` stays as the
/// fallback — a face we could not load is no reason to have no tray icon.
fn build_icon(enabled: bool) -> Icon {
    let ordinal = if enabled { 1 } else { 2 };
    match crate::gui::load_notification_icon(ordinal) {
        Some(handle) => Icon::from_handle(handle),
        None => {
            Icon::from_resource(ordinal, None).expect("icon resources are embedded by build.rs")
        }
    }
}
