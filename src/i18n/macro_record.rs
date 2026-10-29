//! Macro recording: the log lines and the banner texts (ADR 0043).

use super::{Lang, lang};
use winremap::keymap::KeyCombo;

// ---- macro recording (ADR 0043) -------------------------------------------

/// Log line when a recording begins. The count in the banner tells the user
/// how much room is left; this line marks when it started.
pub fn macro_record_started(limit: usize) -> String {
    match lang() {
        Lang::En => format!("macro recording started (up to {limit} commands)"),
        Lang::Ja => format!("マクロの記憶を開始しました（最大 {limit} コマンド）"),
    }
}

pub fn macro_record_stopped(len: usize) -> String {
    match lang() {
        Lang::En => format!("macro recording finished: {len} command(s)"),
        Lang::Ja => format!("マクロの記憶を終了しました: {len} コマンド"),
    }
}

/// The limit ended the recording. Says so rather than letting commands
/// vanish silently (ADR 0043).
pub fn macro_record_truncated(limit: usize) -> String {
    match lang() {
        Lang::En => format!(
            "macro recording stopped at the {limit}-command limit; the first {limit} were kept"
        ),
        Lang::Ja => {
            format!(
                "上限の {limit} コマンドに達したため記憶を終了しました（先頭 {limit} コマンドを保持）"
            )
        }
    }
}

/// An in-progress recording was dropped because the keys that end it may
/// have changed (design doc §5.6).
pub fn macro_record_aborted(reason: &str) -> String {
    match lang() {
        Lang::En => format!("macro recording cancelled ({reason})"),
        Lang::Ja => format!("マクロの記憶を中止しました（{reason}）"),
    }
}

pub fn macro_record_nothing_to_play() -> String {
    match lang() {
        Lang::En => "no macro recorded yet".to_owned(),
        Lang::Ja => "まだマクロを記憶していません".to_owned(),
    }
}

/// The play key pressed while recording, or a record key that means nothing
/// in the current state. Logged so a key that visibly did nothing still
/// leaves a trace.
pub fn macro_record_ignored() -> String {
    match lang() {
        Lang::En => "recording key ignored in the current state".to_owned(),
        Lang::Ja => "現在の状態では意味を持たない記憶キーのため無視しました".to_owned(),
    }
}

pub fn macro_record_replaying(commands: &[KeyCombo]) -> String {
    let steps = commands
        .iter()
        .map(|combo| combo.to_string())
        .collect::<Vec<_>>()
        .join(" → ");
    match lang() {
        Lang::En => format!("replaying the recorded macro ({}): {steps}", commands.len()),
        Lang::Ja => format!(
            "記憶したマクロを再生します（{} コマンド）: {steps}",
            commands.len()
        ),
    }
}

/// The feature could not start. Phrased like the indicator's message: the
/// point is that remapping itself is unaffected.
pub fn macro_record_failed(error: &str) -> String {
    match lang() {
        Lang::En => format!("macro recording unavailable (remapping is unaffected): {error}"),
        Lang::Ja => {
            format!("マクロ記憶機能を利用できません（リマップ動作には影響ありません）: {error}")
        }
    }
}

/// Banner line while recording (design doc §6.3). Carries everything the
/// user needs without looking anything up: how much room is left, which app
/// the keystrokes are going to, and the keys that end and replay it — the
/// last of those because a recording that cannot be ended is the worst way
/// for this feature to fail.
pub fn macro_record_banner_recording(
    len: usize,
    limit: usize,
    app: &str,
    stop_key: &str,
    play_key: &str,
) -> String {
    match lang() {
        Lang::En => format!(
            "Recording macro  {len}/{limit}   in {app}   —   {stop_key} to stop, {play_key} to replay"
        ),
        Lang::Ja => format!(
            "マクロ記憶中  {len}/{limit}   {app} で記憶中   —   {stop_key} で終了 / {play_key} で再生"
        ),
    }
}

/// Banner line when the limit ended the recording.
pub fn macro_record_banner_limit(limit: usize) -> String {
    match lang() {
        Lang::En => format!("Recording stopped — {limit}-command limit reached"),
        Lang::Ja => format!("上限 {limit} コマンドに達したため記憶を終了しました"),
    }
}

/// Banner line during replay: the commands themselves, joined the way the
/// settings window joins a macro's chords.
pub fn macro_record_banner_replaying(app: &str, commands: &[KeyCombo]) -> String {
    let steps = commands
        .iter()
        .map(|combo| combo.to_string())
        .collect::<Vec<_>>()
        .join(" → ");
    match lang() {
        Lang::En => format!("Replaying in {app}:  {steps}"),
        Lang::Ja => format!("{app} で再生中:  {steps}"),
    }
}
