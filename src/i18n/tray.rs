//! The tray: tooltips and what a menu action reports.

use std::path::Path;

use super::{Lang, lang};

pub fn tooltip_status(count: usize) -> String {
    match lang() {
        Lang::En => format!("WinRemap — {count} keymap(s)"),
        Lang::Ja => format!("WinRemap — キーマップ {count} 件"),
    }
}

/// Tray toggle result. Worth a log line: it explains why remapping suddenly
/// stopped, which is the first thing to check when a rule "broke".
pub fn toggle_state(enabled: bool) -> String {
    match (lang(), enabled) {
        (Lang::En, true) => "remapping enabled".to_owned(),
        (Lang::En, false) => "remapping disabled".to_owned(),
        (Lang::Ja, true) => "リマップを有効にしました".to_owned(),
        (Lang::Ja, false) => "リマップを無効にしました".to_owned(),
    }
}

pub fn reload_ok(count: usize) -> String {
    match lang() {
        Lang::En => format!("config reloaded: {count} keymap(s)"),
        Lang::Ja => format!("設定を再読み込みしました（キーマップ {count} 件）"),
    }
}

/// Debug-mode marker for a config load (startup and reload), so the reload
/// timing is visible inside the key-event stream.
pub fn debug_config_loaded(path: &Path, count: usize) -> String {
    match lang() {
        Lang::En => format!("config loaded: {} ({count} keymap(s))", path.display()),
        Lang::Ja => format!(
            "設定ファイルを読み込みました: {}（キーマップ {count} 件）",
            path.display()
        ),
    }
}

/// Message for a failed reload; `error` stays in English on purpose
/// (diagnostics policy above).
pub fn reload_failed(error: &str) -> String {
    match lang() {
        Lang::En => format!("config reload failed, keeping previous config:\n{error}"),
        Lang::Ja => format!("設定の再読み込みに失敗しました。直前の設定を維持します:\n{error}"),
    }
}
