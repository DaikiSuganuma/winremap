//! The settings window: status lines, readings of key notation in human
//! words (screen design §6), and the address bar's actions.

use super::{Lang, lang};
use winremap::keymap::{InputPattern, KeyCombo, Mods, key_name, vk_display_name};

/// The status bar's "running since" segment (v0.4 screen design §5).
pub fn status_started(time: &str) -> String {
    match lang() {
        Lang::En => format!("Started: {time}"),
        Lang::Ja => format!("起動: {time}"),
    }
}

/// Why edit mode could not start (unreadable file). Shown in the status
/// bar; the reason stays technical English (guidelines §11).
pub fn edit_cannot_start(reason: &str) -> String {
    match lang() {
        Lang::En => format!("Cannot start editing: {reason}"),
        Lang::Ja => format!("編集を開始できません: {reason}"),
    }
}

/// A save that failed on I/O or on non-validation grounds (screen design
/// §7.2).
pub fn save_failed(reason: &str) -> String {
    match lang() {
        Lang::En => format!("Could not save: {reason}"),
        Lang::Ja => format!("保存できませんでした: {reason}"),
    }
}

/// A chord in human words: `C-S-h` → "Ctrl + Shift + h". The modifier names
/// are product names, identical in both languages, so only the shape is
/// fixed here (word-order-free by construction — it is a joined list).
pub fn combo_human(combo: &KeyCombo) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (flag, name) in [
        (Mods::CTRL, "Ctrl"),
        (Mods::ALT, "Alt"),
        (Mods::SHIFT, "Shift"),
        (Mods::WIN, "Win"),
    ] {
        if combo.mods.contains(flag) {
            parts.push(name.to_owned());
        }
    }
    // The engraved character where the keyboard has one: this reading sits
    // under the box the user is typing into, and `Ctrl + Oem1` would not tell
    // them they had hit the right key (ADR 0063).
    parts.push(
        key_name(combo.vk, &crate::layout::current()).unwrap_or_else(|| vk_display_name(combo.vk)),
    );
    parts.join(" + ")
}

/// A rule input in human words (screen design §6.1): a chord as
/// `combo_human`, a sequence as "Alt+x, then h (two strokes)".
pub fn input_human(pattern: &InputPattern) -> String {
    match pattern {
        InputPattern::Single(combo) => combo_human(combo),
        InputPattern::Sequence(first, second) => {
            let (first, second) = (combo_human(first), combo_human(second));
            match lang() {
                Lang::En => format!("{first}, then {second} (two strokes)"),
                Lang::Ja => format!("{first} に続けて {second}（2 ストローク）"),
            }
        }
    }
}

/// A rule output in human words: one chord plainly, several as the macro's
/// step sequence.
pub fn output_human(combos: &[KeyCombo]) -> String {
    match combos {
        [single] => combo_human(single),
        several => {
            let steps = several
                .iter()
                .map(combo_human)
                .collect::<Vec<_>>()
                .join(" → ");
            match lang() {
                Lang::En => format!("{steps} (macro)"),
                Lang::Ja => format!("{steps}（マクロ）"),
            }
        }
    }
}

/// The capture button while it counts down (B4, screen design §6.3).
pub fn capture_countdown(seconds: u64) -> String {
    match lang() {
        Lang::En => format!("{seconds}… bring the target app to the front"),
        Lang::Ja => format!("{seconds}… 対象アプリを前面にしてください"),
    }
}

/// An unknown key name with its nearest real one (screen design §6.2).
pub fn unknown_key_suggestion(got: &str, suggest: &str) -> String {
    match lang() {
        Lang::En => format!("unknown key name \"{got}\" — did you mean \"{suggest}\"?"),
        Lang::Ja => format!("未知のキー名 \"{got}\"。\"{suggest}\" のことですか?"),
    }
}

/// The validation footer: how many issues, and the one under the cursor
/// (screen design §6.4). Issue text itself stays technical English.
pub fn issues_found(count: usize, first: &str) -> String {
    match lang() {
        Lang::En => format!("⚠ {count} issue(s): {first}"),
        Lang::Ja => format!("⚠ {count} 件の問題があります: {first}"),
    }
}

/// Logged when the address bar switches the active config file (ADR 0050).
pub fn action_switch_file(name: &str) -> String {
    match lang() {
        Lang::En => format!("Switch config file: {name}"),
        Lang::Ja => format!("設定ファイルを切り替え: {name}"),
    }
}

/// The shell refused to open the config file (no `.toml` association, or the
/// file is gone). Says which file, so the user can open it by hand.
pub fn open_folder_failed(path: &str) -> String {
    match lang() {
        Lang::En => format!(
            "could not open the config folder:
{path}"
        ),
        Lang::Ja => format!(
            "設定フォルダーを開けませんでした:
{path}"
        ),
    }
}

pub fn open_editor_failed(path: &str) -> String {
    match lang() {
        Lang::En => format!(
            "could not open the config file in an editor:
{path}"
        ),
        Lang::Ja => format!(
            "設定ファイルをエディタで開けませんでした:
{path}"
        ),
    }
}

/// Help site URL for the current UI language.
/// The link sits beside the key-notation legend in the settings window, so
/// it goes to the configuration guide rather than the help site's front
/// page — that is where the notation table and the worked examples live.
pub fn help_url() -> &'static str {
    match lang() {
        Lang::En => "https://daikisuganuma.github.io/winremap/config.html",
        Lang::Ja => "https://daikisuganuma.github.io/winremap/ja/config.html",
    }
}

/// Shown under a rule table that contains a macro: says the arrows are an
/// order, and where the pacing between them comes from.
pub fn macro_note(delay_ms: u32) -> String {
    match (lang(), delay_ms) {
        (Lang::En, 0) => "→ marks a macro: the chords are tapped in order, as fast as possible. Add [macro] delay_ms in General to pace them.".to_owned(),
        (Lang::En, delay) => format!("→ marks a macro: the chords are tapped in order, {delay} ms apart ([macro] delay_ms in General)."),
        (Lang::Ja, 0) => "→ はマクロです。各コマンドを順にタップします。間隔は空けません（全体設定の [macro] delay_ms で調整できます）。".to_owned(),
        (Lang::Ja, delay) => format!("→ はマクロです。各コマンドを {delay} ミリ秒間隔で順にタップします（全体設定の [macro] delay_ms）。"),
    }
}

/// Log line for opening the config file in an editor.
pub fn action_open_editor(path: &str) -> String {
    match lang() {
        Lang::En => format!("opening the config file in an editor: {path}"),
        Lang::Ja => format!("設定ファイルをエディタで開きます: {path}"),
    }
}
