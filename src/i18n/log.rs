//! The log's running commentary: what the user did, and what WinRemap
//! decided about each key (ADR 0016, ADR 0057).

use super::{Lang, lang, t};
use winremap::keymap::KeyCombo;

/// A checkbox in the log window. Logged like a tray pick, so that "why did
/// the log stop scrolling" and "why did those lines disappear" are answered
/// by the log itself rather than by remembering what was clicked (owner
/// request 2026-07-29). `label` is the control's own text, so the line names
/// what the user just read on screen.
///
/// One shape for both languages: every word in it is already localized, so a
/// per-language `format!` would only be two identical strings to keep in step.
pub fn action_toggled(label: &str, on: bool) -> String {
    let state = if on { t().config_on } else { t().config_off };
    format!("{label}: {state}")
}

pub fn action_log_cleared() -> String {
    match lang() {
        Lang::En => "log cleared".to_owned(),
        Lang::Ja => "ログを消去しました".to_owned(),
    }
}

/// The count is the point: a paste that looks short can be checked against
/// the number of lines that were actually taken.
pub fn action_log_copied(count: usize) -> String {
    match lang() {
        Lang::En => format!("{count} line(s) copied to the clipboard"),
        Lang::Ja => format!("{count} 行をクリップボードにコピーしました"),
    }
}

/// Log line for closing a window; `window` says which one.
pub fn action_closed(window: &str) -> String {
    match lang() {
        Lang::En => format!("closed: {window}"),
        Lang::Ja => format!("{window} を閉じました"),
    }
}

/// `"A-x u"`-style rendering: a second stroke shows its prefix too.
fn fmt_input(prev: Option<KeyCombo>, input: KeyCombo) -> String {
    let input = combo(&input);
    match prev {
        Some(prefix) => format!("{} {input}", combo(&prefix)),
        None => input,
    }
}

/// Every key in the transcript is named through here rather than through
/// `KeyCombo`'s `Display`, so that a key the config cannot name yet still
/// says what it is instead of printing `0xBF` (ADR 0058).
fn combo(combo: &KeyCombo) -> String {
    crate::keyname::combo(combo)
}

// The lines below carry no `[debug]` prefix: the log window draws the tag in
// its own column and the console adds it back when printing, so that a
// decision and the events under it can be told apart at a glance rather than
// by counting leading spaces (ADR 0057).

pub fn debug_key_pass(input: KeyCombo) -> String {
    let input = combo(&input);
    match lang() {
        Lang::En => format!("{input} → passed through"),
        Lang::Ja => format!("{input} → 素通し"),
    }
}

pub fn debug_key_chord(prev: Option<KeyCombo>, input: KeyCombo, target: KeyCombo) -> String {
    let input = fmt_input(prev, input);
    let target = combo(&target);
    match lang() {
        Lang::En => format!("{input} → remapped to {target}"),
        Lang::Ja => format!("{input} → {target} に置換"),
    }
}

pub fn debug_key_substituted(input: KeyCombo, target_vk: u16) -> String {
    let input = combo(&input);
    let target = crate::keyname::key(target_vk);
    match lang() {
        Lang::En => format!("{input} → substituted with {target} (bare-key rule)"),
        Lang::Ja => format!("{input} → {target} に差し替え（単キールール）"),
    }
}

pub fn debug_key_macro(
    prev: Option<KeyCombo>,
    input: KeyCombo,
    strokes: u8,
    steps: &str,
) -> String {
    let input = fmt_input(prev, input);
    match lang() {
        Lang::En => format!("{input} → macro executed ({strokes} strokes: {steps})"),
        Lang::Ja => format!("{input} → マクロ実行（{strokes} ストローク: {steps}）"),
    }
}

pub fn debug_key_repeat(input: KeyCombo) -> String {
    let input = combo(&input);
    match lang() {
        Lang::En => format!("{input} → auto-repeat (suppressed)"),
        Lang::Ja => format!("{input} → キーリピート（抑止）"),
    }
}

/// A physical press or release, as it reached the hook. The key name comes
/// first so a column of these lines reads down the page; the modifiers held
/// at the time are deliberately left out, because the decision line above
/// already spells the combination out and repeating it here would bury the
/// one thing this line adds — that the event happened at all.
pub fn debug_physical(vk: u16, up: bool) -> String {
    format!("{} {}", crate::keyname::key(vk), arrow(up))
}

/// Echo of an injected event passing through the hook. `source` is one of
/// the pre-localized `debug_source_*` labels.
pub fn debug_injected(vk: u16, up: bool, source: &str) -> String {
    let key = crate::keyname::key(vk);
    let arrow = arrow(up);
    match lang() {
        Lang::En => format!("{key} {arrow} ({source})"),
        Lang::Ja => format!("{key} {arrow}（{source}）"),
    }
}

/// Up and down, in the one notation both languages share.
fn arrow(up: bool) -> &'static str {
    if up { "↑" } else { "↓" }
}

pub fn debug_key_prefix(input: KeyCombo) -> String {
    let input = combo(&input);
    match lang() {
        Lang::En => format!("{input} → prefix armed (waiting for the next key)"),
        Lang::Ja => format!("{input} → プレフィックス待機（次のキーで確定）"),
    }
}

pub fn debug_key_swallowed(prev: Option<KeyCombo>, input: KeyCombo) -> String {
    let input = fmt_input(prev, input);
    match lang() {
        Lang::En => format!("{input} → undefined sequence (swallowed)"),
        Lang::Ja => format!("{input} → 未定義のシーケンス（握りつぶし）"),
    }
}

pub fn debug_events_dropped(count: u32) -> String {
    match lang() {
        Lang::En => format!("{count} events dropped — buffer full"),
        Lang::Ja => format!("バッファ超過により {count} 件のイベントを省略しました"),
    }
}

/// The foreground report's one visible line: the value to write in a keymap's
/// `application` list, and which of the configured keymaps that value reaches.
/// Together they answer "why is my rule not firing here", which is what this
/// line is for; `keymap_list` is pre-joined by the caller.
///
/// The full path goes on a line of its own (`Kind::Detail`), because it is
/// long, it is rarely the answer, and putting it first pushed the two things
/// that are off the right-hand edge of the window (ADR 0058).
pub fn debug_foreground(app_name: &str, keymap_list: &str) -> String {
    match lang() {
        Lang::En => format!("application = \"{app_name}\" — matching keymaps: {keymap_list}"),
        Lang::Ja => {
            format!("application 指定値: \"{app_name}\" — 適用されるキーマップ: {keymap_list}")
        }
    }
}
