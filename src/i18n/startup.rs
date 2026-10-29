//! What the session says as it starts and ends: the config it loaded and
//! why, the banners, and the failures that end a start.

use std::path::Path;

use super::{Lang, lang};

pub fn startup_loaded(count: usize, path: &Path) -> String {
    let version = env!("CARGO_PKG_VERSION");
    match lang() {
        Lang::En => format!(
            "WinRemap {version}: {count} keymap(s) loaded from {}",
            path.display()
        ),
        Lang::Ja => format!(
            "WinRemap {version}: {} からキーマップを {count} 件読み込みました",
            path.display()
        ),
    }
}

/// Announces the config file written on first run. Said out loud rather than
/// done silently: the file is the thing the user edits next, so the session
/// has to name where it landed.
pub fn created_default_config(path: &Path) -> String {
    match lang() {
        Lang::En => format!("created a default config at {}", path.display()),
        Lang::Ja => format!("既定の設定ファイルを作成しました: {}", path.display()),
    }
}

/// Why this run opened a file nobody asked for on the command line: the
/// settings window chose it, in some earlier run (ADR 0077). Worth a line —
/// the keymaps in force are otherwise unexplained to anyone reading the log
/// months later.
pub fn config_from_last_time(path: &Path) -> String {
    match lang() {
        Lang::En => format!(
            "opening the config file chosen last time: {}",
            path.display()
        ),
        Lang::Ja => format!("前回選ばれた設定ファイルを開きます: {}", path.display()),
    }
}

/// ...and when that file has been renamed or deleted since. The default is
/// used, and the name that went missing is said: without it, the user sees
/// their keymaps quietly revert with nothing to search for.
pub fn config_from_last_time_gone(path: &Path) -> String {
    match lang() {
        Lang::En => format!(
            "the config file chosen last time is gone ({}); using the default",
            path.display()
        ),
        Lang::Ja => format!(
            "前回選ばれた設定ファイルがありません（{}）。既定の設定ファイルを使います",
            path.display()
        ),
    }
}

/// The switch worked but could not be recorded, so the next start will open
/// the old file. Reported for exactly that reason (ADR 0077).
pub fn remember_config_failed(reason: &str) -> String {
    match lang() {
        Lang::En => format!("could not record which config file to open next time: {reason}"),
        Lang::Ja => format!("次回開く設定ファイルを記録できませんでした: {reason}"),
    }
}

/// Opens the log: when this run of WinRemap started, and which build it is.
/// The version is repeated here (rather than only in `startup_loaded`) because
/// this is the line a pasted log is read from.
pub fn session_started(now: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    match lang() {
        Lang::En => format!("{now}  WinRemap v{version} started"),
        Lang::Ja => format!("{now}  WinRemap v{version} を起動しました"),
    }
}

/// The second line the log window opens with: which config file is loaded
/// right now, and how many keymaps came out of it.
///
/// The startup line saying this reaches a terminal, and the window is usually
/// opened long after launch — so without this the one place that never said
/// which file was in force was the window built for answering that kind of
/// question (v0.5 plan §4 item 5).
pub fn log_active_config(path: &Path, count: usize) -> String {
    match lang() {
        Lang::En => format!("config: {} ({count} keymap(s))", path.display()),
        Lang::Ja => format!("設定ファイル: {}（キーマップ {count} 件）", path.display()),
    }
}

/// Closes it, on the way out of `run`.
pub fn session_ended(now: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    match lang() {
        Lang::En => format!("{now}  WinRemap v{version} exited"),
        Lang::Ja => format!("{now}  WinRemap v{version} を終了しました"),
    }
}

/// The GUI could not start (e.g. no usable GPU adapter). Remapping is
/// unaffected, so the message says so rather than sounding fatal. `error`
/// stays in English on purpose (diagnostics policy above).
pub fn gui_failed(error: &str) -> String {
    match lang() {
        Lang::En => {
            format!("could not open the window (remapping is unaffected):\n{error}")
        }
        Lang::Ja => {
            format!("ウィンドウを開けませんでした（リマップ動作には影響ありません）:\n{error}")
        }
    }
}
