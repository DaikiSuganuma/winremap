//! User-facing UI text in English and Japanese (ADR 0014).
//!
//! Everything a user reads — tray menu, tooltips, console guidance, CLI help
//! — goes through this module; never hardcode UI strings elsewhere. Technical
//! diagnostics (config validation errors, anyhow contexts) intentionally stay
//! English so they can be pasted into issue reports verbatim.
//!
//! When adding a message, define BOTH languages — a missing translation is a
//! review blocker (guidelines §11).

//!
//! One file per audience (v1.1 plan D2): `startup` for the lines the
//! session begins and ends with, `cli` for `--help` and argument errors,
//! `tray` for the menu's reports, `config_window` for the settings window,
//! `log` for the running commentary, `ime` for the indicator and the cursor,
//! `macro_record` for recording. `en` and `ja` hold the static [`Texts`]
//! tables. Every formatted message still decides both languages in one
//! `match`, which is what keeps a missing translation a compile error.

mod cli;
mod config_window;
mod en;
mod ime;
mod ja;
mod log;
mod macro_record;
mod startup;
#[cfg(test)]
mod tests;
mod tray;

pub use cli::*;
pub use config_window::*;
pub use ime::*;
pub use log::*;
pub use macro_record::*;
pub use startup::*;
pub use tray::*;

use std::sync::OnceLock;

use en::EN;
use ja::JA;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Ja,
}

static LANG: OnceLock<Lang> = OnceLock::new();

/// Picks the UI language once at startup: explicit `--lang` wins, otherwise
/// the system UI locale (`ja*` → Japanese, anything else → English).
pub fn init(override_lang: Option<Lang>) {
    let lang = override_lang.unwrap_or_else(|| match sys_locale::get_locale() {
        Some(locale) if locale.to_ascii_lowercase().starts_with("ja") => Lang::Ja,
        _ => Lang::En,
    });
    let _ = LANG.set(lang);
}

pub(super) fn lang() -> Lang {
    // English fallback covers early errors emitted before init() runs.
    LANG.get().copied().unwrap_or(Lang::En)
}

/// Static UI strings. Formatted messages live as functions below.
pub struct Texts {
    /// Dialog caption. Product name is spelled WinRemap in UI text (ADR 0025).
    pub app_name: &'static str,
    pub menu_enabled: &'static str,
    pub menu_reload: &'static str,
    pub menu_settings: &'static str,
    pub menu_log: &'static str,
    pub menu_quit: &'static str,
    pub config_window_title: &'static str,
    pub config_window_open_in_editor: &'static str,
    /// Opens the config folder in Explorer (address-bar dropdown).
    pub config_open_folder: &'static str,
    /// Enters edit mode (v0.4 screen design §2).
    pub config_edit: &'static str,
    pub config_save: &'static str,
    pub config_revert: &'static str,
    pub config_close: &'static str,
    pub config_cancel: &'static str,
    /// Tooltip on the address bar's inert dropdown while editing.
    pub config_switch_locked: &'static str,
    /// Saving over an externally changed file (screen design §7.1).
    pub config_overwrite: &'static str,
    pub config_reread: &'static str,
    pub config_next_issue: &'static str,
    /// The breadcrumb-row banner while editing (screen design §4.1).
    pub config_edit_notice: &'static str,
    pub config_close_confirm: &'static str,
    pub config_external_changed: &'static str,
    pub config_add_app: &'static str,
    /// Replaces the application list with `"*"` (owner decision 2026-07-26).
    pub config_target_all_apps: &'static str,
    /// Starts the foreground-capture countdown (B4, screen design §6.3).
    pub config_capture_app: &'static str,
    pub config_add_rule: &'static str,
    pub config_keymap_add: &'static str,
    pub config_keymap_remove: &'static str,
    pub config_move_up: &'static str,
    pub config_move_down: &'static str,
    pub status_saved: &'static str,
    /// The key-name reference popup (screen design §6.5).
    pub config_keys_title: &'static str,
    pub config_keys_mods: &'static str,
    pub config_keys_chars: &'static str,
    pub config_keys_chars_list: &'static str,
    pub config_keys_function: &'static str,
    pub config_keys_function_list: &'static str,
    pub config_keys_special: &'static str,
    /// The symbol-key row. Its contents are read off the attached keyboard
    /// rather than translated, so only the label and the footnote are here
    /// (ADR 0063).
    pub config_keys_symbols: &'static str,
    pub config_keys_symbols_note: &'static str,
    /// Shown in place of the row when the keyboard could not be read.
    pub config_keys_symbols_unknown: &'static str,
    /// Tooltip on the ● change mark: the file differs from what is loaded.
    pub config_file_changed: &'static str,
    /// The breadcrumb's first segment (v0.4 screen design §4.1).
    pub config_breadcrumb_root: &'static str,
    /// Status-bar message after any successful load.
    pub status_loaded: &'static str,
    pub config_window_no_config: &'static str,
    pub config_general: &'static str,
    pub config_keymaps: &'static str,
    pub config_no_keymaps: &'static str,
    pub config_no_rules: &'static str,
    pub config_none: &'static str,
    pub config_on: &'static str,
    pub config_off: &'static str,
    pub config_apps_all: &'static str,
    pub config_field_name: &'static str,
    pub config_field_apps: &'static str,
    pub config_field_exclude: &'static str,
    pub config_rules: &'static str,
    /// Named for the direction of travel, not just "input"/"output": which
    /// side of the remap a column shows is the first thing people get wrong.
    pub config_rule_input: &'static str,
    pub config_rule_output: &'static str,
    pub config_rule_comment: &'static str,
    /// Other keymaps binding the same input. Only shown when there are any.
    pub config_rule_shared: &'static str,
    pub config_rule_shared_note: &'static str,
    pub config_column_item: &'static str,
    pub config_column_key: &'static str,
    pub config_column_value: &'static str,
    pub config_apps_case_note: &'static str,
    pub config_apps_all_note: &'static str,
    pub config_column_app: &'static str,
    /// Leads a line WinRemap wrote itself, so it reads apart from the user's
    /// own comments. Japanese has a mark for exactly this; English does not.
    pub note_marker: &'static str,
    pub config_macro_section: &'static str,
    pub config_notation_title: &'static str,
    pub config_notation_ctrl: &'static str,
    pub config_notation_alt: &'static str,
    pub config_notation_shift: &'static str,
    pub config_notation_win: &'static str,
    pub config_notation_sequence: &'static str,
    pub config_notation_macro: &'static str,
    pub config_help_link: &'static str,
    pub config_macro_delay: &'static str,
    /// `[macro]` recording rows (ADR 0043).
    pub config_macro_record_start: &'static str,
    pub config_macro_record_stop: &'static str,
    pub config_macro_record_play: &'static str,
    /// The macro held in memory right now — not something in the file, which
    /// is why the row says so.
    pub config_macro_recorded: &'static str,
    pub config_macro_recorded_note: &'static str,
    pub config_ime_indicator: &'static str,
    pub config_ime_enabled: &'static str,
    pub config_ime_duration: &'static str,
    pub config_ime_size: &'static str,
    pub config_ime_opacity: &'static str,
    pub config_ime_show_app_name: &'static str,
    pub config_ime_triggers: &'static str,
    pub config_ime_change_cursor_color: &'static str,
    pub config_ime_cursor_color: &'static str,
    pub config_ime_cursor_color_hint: &'static str,
    /// Marks a line the user caused, so actions stand out from the key
    /// traffic. Also the tag drawn in the log window's gutter.
    pub log_action_prefix: &'static str,
    /// Gutter tags for the debug transcript. A physical key event, what
    /// WinRemap decided about it, and what it then put on the wire.
    pub log_tag_input: &'static str,
    pub log_tag_decision: &'static str,
    pub log_tag_injected: &'static str,
    /// The application in front, which is what decides whose keymap applies —
    /// the reason a rule that "stopped working" usually stopped working.
    pub log_tag_window: &'static str,
    /// The IME indicator's own polling, which belongs to no key.
    pub log_tag_ime: &'static str,
    /// Why an in-progress macro recording was dropped (design doc §5.6).
    pub macro_record_reason_reload: &'static str,
    pub macro_record_reason_disabled: &'static str,
    /// Stands in for the app name when the foreground window cannot be
    /// identified (an elevated window denies the query under UIPI).
    pub macro_record_unknown_app: &'static str,
    pub log_window_title: &'static str,
    pub log_window_hint: &'static str,
    pub log_window_follow: &'static str,
    pub log_window_detailed: &'static str,
    pub log_window_detailed_hint: &'static str,
    pub log_window_clear: &'static str,
    pub log_window_copy: &'static str,
    pub tooltip_disabled: &'static str,
    pub tooltip_reload_failed: &'static str,
    pub remapping_active: &'static str,
    pub already_running: &'static str,
    pub debug_console_wait: &'static str,
    pub debug_none: &'static str,
    pub debug_foreground_unknown: &'static str,
    pub debug_source_remap: &'static str,
    pub debug_source_compensation: &'static str,
    pub debug_source_external: &'static str,
    pub debug_ime_shell_skip: &'static str,
}

pub fn t() -> &'static Texts {
    match lang() {
        Lang::En => &EN,
        Lang::Ja => &JA,
    }
}
