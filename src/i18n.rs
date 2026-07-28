//! User-facing UI text in English and Japanese (ADR 0014).
//!
//! Everything a user reads — tray menu, tooltips, console guidance, CLI help
//! — goes through this module; never hardcode UI strings elsewhere. Technical
//! diagnostics (config validation errors, anyhow contexts) intentionally stay
//! English so they can be pasted into issue reports verbatim.
//!
//! When adding a message, define BOTH languages — a missing translation is a
//! review blocker (guidelines §11).

use std::path::Path;
use std::sync::OnceLock;

use winremap::keymap::{InputPattern, KeyCombo, Mods, vk_display_name};

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

fn lang() -> Lang {
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
    /// Marks a line the user caused, so actions stand out from the key
    /// traffic. Also the tag drawn in the log window's gutter.
    pub log_action_prefix: &'static str,
    /// Gutter tags for the debug transcript. A physical key event, what
    /// WinRemap decided about it, and what it then put on the wire.
    pub log_tag_input: &'static str,
    pub log_tag_decision: &'static str,
    pub log_tag_injected: &'static str,
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
    pub debug_none: &'static str,
    pub debug_foreground_unknown: &'static str,
    pub debug_source_remap: &'static str,
    pub debug_source_compensation: &'static str,
    pub debug_source_external: &'static str,
    pub debug_ime_shell_skip: &'static str,
}

static EN: Texts = Texts {
    app_name: "WinRemap",
    menu_enabled: "Enabled",
    menu_reload: "Reload config",
    menu_settings: "Settings",
    menu_log: "Show log",
    menu_quit: "Quit",
    config_window_title: "WinRemap — settings",
    config_window_open_in_editor: "Open in text editor",
    config_open_folder: "Open folder",
    config_edit: "Edit",
    config_save: "Save",
    config_revert: "Revert",
    config_close: "Close",
    config_cancel: "Cancel",
    config_switch_locked: "Files cannot be switched while editing",
    config_overwrite: "Overwrite",
    config_reread: "Re-read (discard edits)",
    config_next_issue: "Next ▸",
    config_edit_notice: "● Editing — the file is untouched until you press Save",
    config_close_confirm: "Close without saving?",
    config_external_changed: "The config file was changed outside WinRemap.",
    config_add_app: "Add application",
    config_target_all_apps: "Target all applications",
    config_capture_app: "Capture the foreground app",
    config_add_rule: "Add rule",
    config_keymap_add: "Add keymap",
    config_keymap_remove: "Delete keymap",
    config_move_up: "Move up",
    config_move_down: "Move down",
    status_saved: "Saved.",
    config_keys_title: "Key names",
    config_keys_mods: "Modifiers",
    config_keys_chars: "Characters",
    config_keys_chars_list: "a–z 0–9",
    config_keys_function: "Function",
    config_keys_function_list: "F1–F24",
    config_keys_special: "Special",
    config_file_changed: "Changed on disk — not loaded yet",
    config_breadcrumb_root: "Settings",
    status_loaded: "Config loaded.",
    config_window_no_config: "No config is loaded.",
    config_general: "General",
    config_keymaps: "Keymaps",
    config_no_keymaps: "(no keymaps)",
    config_no_rules: "(no rules)",
    config_none: "(none)",
    config_on: "on",
    config_off: "off",
    config_apps_all: "* (all applications)",
    config_field_name: "Name",
    config_field_apps: "Applications",
    config_field_exclude: "Excluded applications",
    config_rules: "Remap rules",
    config_rule_input: "WinRemap input",
    config_rule_output: "Output to the app",
    config_rule_comment: "Comment",
    config_rule_shared: "Also bound in",
    config_rule_shared_note: "Only one keymap can win: an app-specific keymap beats a \"*\" one, and among equals the one defined first wins.",
    config_column_item: "Setting",
    config_column_key: "Key",
    config_column_value: "Value",
    config_apps_case_note: "Matched against the exe name, ignoring upper/lower case.",
    config_apps_all_note: "Every application, minus the exclusions below.",
    config_column_app: "Application",
    note_marker: "\u{2139}",
    config_macro_section: "Macros",
    config_notation_title: "Key notation",
    config_notation_ctrl: "Ctrl",
    config_notation_alt: "Alt",
    config_notation_shift: "Shift",
    config_notation_win: "Windows key",
    config_notation_sequence: "A space means two strokes: \"A-x h\" is Alt+X, then H.",
    config_notation_macro: "Arrows mean a macro: each chord is tapped in order, one key press.",
    config_help_link: "Open the configuration guide",
    config_macro_delay: "Macro delay (ms)",
    config_macro_record_start: "Start recording",
    config_macro_record_stop: "Stop recording",
    config_macro_record_play: "Replay",
    config_macro_recorded: "Recorded macro",
    config_macro_recorded_note: "In memory only — not in the file, and gone when WinRemap exits.",
    config_ime_indicator: "IME status indicator",
    config_ime_enabled: "Enabled",
    config_ime_duration: "Duration (ms)",
    config_ime_size: "Size (px)",
    config_ime_opacity: "Opacity",
    config_ime_show_app_name: "Show app name",
    config_ime_triggers: "Trigger keys",
    log_action_prefix: "[action]",
    log_tag_input: "[input]",
    log_tag_decision: "[decided]",
    log_tag_injected: "[injected]",
    macro_record_reason_reload: "config reloaded",
    macro_record_reason_disabled: "remapping disabled",
    macro_record_unknown_app: "an unknown app",
    log_window_title: "WinRemap — log",
    log_window_hint: "Debug logging is on while this window is open. Press keys to see how they are handled.",
    log_window_follow: "Follow newest",
    log_window_detailed: "Every event",
    log_window_detailed_hint: "Off: one line per key, saying what WinRemap decided.\nOn: the whole stream — every physical press and release, and every event WinRemap sent in reply.\n\nThe two halves of a remap happen at different moments: the target is pressed when you press the key and released when you let go. The time column is what shows that.",
    log_window_clear: "Clear",
    log_window_copy: "Copy all",
    tooltip_disabled: "WinRemap (disabled)",
    tooltip_reload_failed: "WinRemap — config reload FAILED (previous config still active)",
    remapping_active: "remapping active. Use the tray icon to reload or quit.",
    already_running: "WinRemap is already running (check the task tray)",
    debug_none: "(none)",
    debug_foreground_unknown: "[debug] foreground: could not determine (possibly an elevated window)",
    debug_source_remap: "remap",
    debug_source_compensation: "modifier adjust",
    debug_source_external: "EXTERNAL software",
    debug_ime_shell_skip: "[debug] IME indicator: shell surface (taskbar/desktop) → ignored",
};

static JA: Texts = Texts {
    app_name: "WinRemap",
    menu_enabled: "有効",
    menu_reload: "設定を再読み込み",
    menu_settings: "設定",
    menu_log: "ログを表示",
    menu_quit: "終了",
    config_window_title: "WinRemap — 設定",
    config_window_open_in_editor: "テキストエディタで開く",
    config_open_folder: "フォルダーを開く",
    config_edit: "編集",
    config_save: "保存",
    config_revert: "元に戻す",
    config_close: "閉じる",
    config_cancel: "キャンセル",
    config_switch_locked: "編集中はファイルを切り替えられません",
    config_overwrite: "上書き保存",
    config_reread: "読み直す（編集を破棄）",
    config_next_issue: "次へ ▸",
    config_edit_notice: "● 編集中 — ［保存］を押すまで設定ファイルは変わりません",
    config_close_confirm: "保存せずに閉じますか?",
    config_external_changed: "設定ファイルが WinRemap の外で変更されています。",
    config_add_app: "アプリを追加",
    config_target_all_apps: "全アプリを対象とする",
    config_capture_app: "今の前面アプリから取得",
    config_add_rule: "規則を追加",
    config_keymap_add: "キーマップを追加",
    config_keymap_remove: "キーマップを削除",
    config_move_up: "上へ",
    config_move_down: "下へ",
    status_saved: "保存しました",
    config_keys_title: "使えるキー名",
    config_keys_mods: "修飾",
    config_keys_chars: "文字",
    config_keys_chars_list: "a〜z 0〜9",
    config_keys_function: "ファンクション",
    config_keys_function_list: "F1〜F24",
    config_keys_special: "特殊",
    config_file_changed: "ディスク上で変更されています（未読み込み）",
    config_breadcrumb_root: "設定",
    status_loaded: "読み込み完了しました",
    config_window_no_config: "設定が読み込まれていません。",
    config_general: "全体設定",
    config_keymaps: "キーマップ",
    config_no_keymaps: "（キーマップなし）",
    config_no_rules: "（規則なし）",
    config_none: "（なし）",
    config_on: "オン",
    config_off: "オフ",
    config_apps_all: "*（全アプリ）",
    config_field_name: "名前",
    config_field_apps: "対象アプリ",
    config_field_exclude: "除外アプリ",
    config_rules: "リマップ規則",
    config_rule_input: "WinRemap 入力",
    config_rule_output: "アプリ向け出力",
    config_rule_comment: "コメント",
    config_rule_shared: "他のキーマップ",
    config_rule_shared_note: "適用されるのは 1 つだけです。アプリ指定のキーマップが \"*\" より優先され、同じ種類なら先に書いた方が優先されます。",
    config_column_item: "設定",
    config_column_key: "キー",
    config_column_value: "値",
    config_apps_case_note: "exe 名で照合します。大文字・小文字は区別しません。",
    config_apps_all_note: "下の除外アプリを除く、すべてのアプリが対象です。",
    config_column_app: "アプリ",
    note_marker: "\u{203b}",
    config_macro_section: "マクロ",
    config_notation_title: "キー記法について",
    config_notation_ctrl: "Ctrl キー",
    config_notation_alt: "Alt キー",
    config_notation_shift: "Shift キー",
    config_notation_win: "Windows キー",
    config_notation_sequence: "空白は 2 ストロークです。\"A-x h\" は Alt+X を押してから H を押します。",
    config_notation_macro: "矢印はマクロです。1 回のキー入力で、各コマンドを順にタップします。",
    config_help_link: "設定ガイドを開く",
    config_macro_delay: "マクロ間隔（ミリ秒）",
    config_macro_record_start: "記憶開始",
    config_macro_record_stop: "記憶終了",
    config_macro_record_play: "再生",
    config_macro_recorded: "記憶したマクロ",
    config_macro_recorded_note: "メモリ上だけの内容です。設定ファイルには書かれず、WinRemap を終了すると消えます。",
    config_ime_indicator: "IME 状態インジケーター",
    config_ime_enabled: "有効",
    config_ime_duration: "表示時間（ミリ秒）",
    config_ime_size: "サイズ（px）",
    config_ime_opacity: "不透明度",
    config_ime_show_app_name: "アプリ名を表示",
    config_ime_triggers: "トリガーキー",
    log_action_prefix: "[操作]",
    log_tag_input: "[入力]",
    log_tag_decision: "[判定]",
    log_tag_injected: "[注入]",
    macro_record_reason_reload: "設定をリロードしたため",
    macro_record_reason_disabled: "リマップを無効にしたため",
    macro_record_unknown_app: "不明なアプリ",
    log_window_title: "WinRemap — ログ",
    log_window_hint: "このウィンドウを開いている間、デバッグログを記録します。キーを押すと処理内容が表示されます。",
    log_window_follow: "最新に追従",
    log_window_detailed: "全イベント",
    log_window_detailed_hint: "オフ: キー 1 つにつき 1 行。WinRemap が何をすると判定したかだけを出します。\nオン: 流れた入力をすべて出します。物理キーの押下・解放と、それに対して WinRemap が送出したイベントの全部です。\n\nリマップは 2 つの時刻に分かれて起きます。置換先はキーを押した時に押され、離した時に離されます。時刻の列はそれを見分けるためのものです。",
    log_window_clear: "消去",
    log_window_copy: "全体をコピー",
    tooltip_disabled: "WinRemap（無効）",
    tooltip_reload_failed: "WinRemap — 設定の再読み込みに失敗（前の設定で動作中）",
    remapping_active: "リマップ稼働中。再読み込み・終了はトレイアイコンから。",
    already_running: "WinRemap は既に起動しています（タスクトレイを確認してください）",
    debug_none: "（なし）",
    debug_foreground_unknown: "[debug] 前面アプリ: 取得できませんでした（管理者権限ウィンドウの可能性）",
    debug_source_remap: "置換",
    debug_source_compensation: "修飾補正",
    debug_source_external: "外部ソフト",
    debug_ime_shell_skip: "[debug] IME インジケーター: シェル面（タスクバー/デスクトップ）→ 無視",
};

pub fn t() -> &'static Texts {
    match lang() {
        Lang::En => &EN,
        Lang::Ja => &JA,
    }
}

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
    parts.push(vk_display_name(combo.vk));
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

/// Closes it, on the way out of `run`.
pub fn session_ended(now: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    match lang() {
        Lang::En => format!("{now}  WinRemap v{version} exited"),
        Lang::Ja => format!("{now}  WinRemap v{version} を終了しました"),
    }
}

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
/// timing is visible inside the `[debug]` key-event stream.
pub fn debug_config_loaded(path: &Path, count: usize) -> String {
    match lang() {
        Lang::En => format!(
            "[debug] config loaded: {} ({count} keymap(s))",
            path.display()
        ),
        Lang::Ja => format!(
            "[debug] 設定ファイルを読み込みました: {}（キーマップ {count} 件）",
            path.display()
        ),
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

/// Log line for closing a window; `window` says which one.
pub fn action_closed(window: &str) -> String {
    match lang() {
        Lang::En => format!("closed: {window}"),
        Lang::Ja => format!("{window} を閉じました"),
    }
}

/// Log line for opening the config file in an editor.
pub fn action_open_editor(path: &str) -> String {
    match lang() {
        Lang::En => format!("opening the config file in an editor: {path}"),
        Lang::Ja => format!("設定ファイルをエディタで開きます: {path}"),
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

/// Indicator-thread debug line: one query outcome and what was done.
/// `via_child` marks answers that came from a child window's input thread
/// rather than the foreground window's own (ADR 0033), so detection issues in
/// UWP and WinUI 3 apps are visible in reports.
pub fn debug_ime_query(open: Option<bool>, shown: bool, via_child: bool) -> String {
    match lang() {
        Lang::En => {
            let state = match open {
                Some(true) => "ON",
                Some(false) => "OFF",
                None => "unknown",
            };
            let action = if shown { "panel shown" } else { "no panel" };
            let via = if via_child { " (via child window)" } else { "" };
            format!("[debug] IME indicator: state={state} → {action}{via}")
        }
        Lang::Ja => {
            let state = match open {
                Some(true) => "オン",
                Some(false) => "オフ",
                None => "不明",
            };
            let action = if shown {
                "パネル表示"
            } else {
                "表示なし"
            };
            let via = if via_child {
                "（子ウィンドウ経由）"
            } else {
                ""
            };
            format!("[debug] IME インジケーター: 状態={state} → {action}{via}")
        }
    }
}

/// The IME indicator could not start (or died); remapping keeps running.
/// `error` stays in English on purpose (diagnostics policy above).
pub fn ime_indicator_failed(error: &str) -> String {
    match lang() {
        Lang::En => format!("IME indicator unavailable (remapping is unaffected): {error}"),
        Lang::Ja => {
            format!("IME インジケーターを利用できません（リマップ動作には影響ありません）: {error}")
        }
    }
}

pub fn no_config_file(path: &Path) -> String {
    match lang() {
        Lang::En => format!(
            "no config file at {}.\nCreate it (see examples/minimal.toml) or pass --config <path>.",
            path.display()
        ),
        Lang::Ja => format!(
            "設定ファイルがありません: {}\nexamples/minimal.toml を参考に作成するか、--config <path> を指定してください。",
            path.display()
        ),
    }
}

/// Startup line for a `test-inject` build running with `--accept-injected`
/// (ADR 0053). The mode is invisible in normal use — the keyboard behaves as
/// always until another program injects input — so it is announced instead.
pub fn test_build_notice() -> &'static str {
    match lang() {
        Lang::En => {
            "TEST BUILD: input injected by other software is remapped too (--accept-injected)"
        }
        Lang::Ja => "テストビルド: 他ソフトが注入した入力もリマップします（--accept-injected）",
    }
}

/// Appended to the tray menu's caption in the same mode, so the running
/// instance identifies itself without opening the log.
pub fn test_build_tray_suffix() -> &'static str {
    match lang() {
        Lang::En => " — TEST BUILD",
        Lang::Ja => " — テストビルド",
    }
}

pub fn unknown_argument(arg: &str) -> String {
    match lang() {
        Lang::En => format!("unknown argument `{arg}` (try --help)"),
        Lang::Ja => format!("不明な引数 `{arg}` です（--help を参照）"),
    }
}

/// `"A-x u"`-style rendering: a second stroke shows its prefix too.
fn fmt_input(prev: Option<KeyCombo>, input: KeyCombo) -> String {
    match prev {
        Some(prefix) => format!("{prefix} {input}"),
        None => input.to_string(),
    }
}

// The lines below carry no `[debug]` prefix: the log window draws the tag in
// its own column and the console adds it back when printing, so that a
// decision and the events under it can be told apart at a glance rather than
// by counting leading spaces (ADR 0057).

pub fn debug_key_pass(input: KeyCombo) -> String {
    match lang() {
        Lang::En => format!("{input} → passed through"),
        Lang::Ja => format!("{input} → 素通し"),
    }
}

pub fn debug_key_chord(prev: Option<KeyCombo>, input: KeyCombo, target: KeyCombo) -> String {
    let input = fmt_input(prev, input);
    match lang() {
        Lang::En => format!("{input} → remapped to {target}"),
        Lang::Ja => format!("{input} → {target} に置換"),
    }
}

pub fn debug_key_substituted(input: KeyCombo, target_vk: u16) -> String {
    let target = vk_display_name(target_vk);
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
    format!("{} {}", vk_display_name(vk), arrow(up))
}

/// Echo of an injected event passing through the hook. `source` is one of
/// the pre-localized `debug_source_*` labels.
pub fn debug_injected(vk: u16, up: bool, source: &str) -> String {
    let key = vk_display_name(vk);
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
        Lang::En => format!("[debug] ({count} events dropped — buffer full)"),
        Lang::Ja => format!("[debug] （バッファ超過により {count} 件のイベントを省略）"),
    }
}

/// Debug-mode foreground report. `app_name` is exactly what belongs in the
/// config's `application` list; `keymap_list` is pre-joined by the caller.
pub fn debug_foreground(full_path: &str, app_name: &str, keymap_list: &str) -> String {
    match lang() {
        Lang::En => format!(
            "[debug] foreground: {full_path}\n        application = \"{app_name}\"\n        matching keymaps: {keymap_list}"
        ),
        Lang::Ja => format!(
            "[debug] 前面アプリ: {full_path}\n        application 指定値: \"{app_name}\"\n        適用されるキーマップ: {keymap_list}"
        ),
    }
}

pub fn help_text() -> String {
    let mut text = base_help_text();
    // Documented only where it is accepted; ordinary builds reject the flag.
    if let Some(extra) = test_inject_help() {
        text.push_str(extra);
    }
    text
}

/// The test-only flag's help entry, present only in `test-inject` builds
/// (ADR 0053).
fn test_inject_help() -> Option<&'static str> {
    #[cfg(feature = "test-inject")]
    {
        Some(match lang() {
            Lang::En => {
                "
        --accept-injected  Remap input injected by other software as well
                           (TEST BUILD ONLY — for UI test automation)"
            }
            Lang::Ja => {
                "
        --accept-injected  他ソフトが注入した入力もリマップする
                           （テストビルド専用 — UI テスト自動化用）"
            }
        })
    }
    #[cfg(not(feature = "test-inject"))]
    None
}

fn base_help_text() -> String {
    let version = env!("CARGO_PKG_VERSION");
    match lang() {
        Lang::En => format!(
            "WinRemap {version} — per-application key remapper for Windows

USAGE:
    winremap [OPTIONS]

OPTIONS:
    -c, --config <PATH>    Config file (default: %APPDATA%\\winremap\\config.toml)
        --lang <en|ja>     UI language (default: system language)
        --debug            Print foreground-app and key-decision info
        --macro-delay <MS> Pause between macro strokes, 0-15 ms (default 0;
                           try 5-10 if macros misfire in some apps)
    -V, --version          Print version
    -h, --help             Print this help"
        ),
        Lang::Ja => format!(
            "WinRemap {version} — Windows 用アプリ別キーリマッパー

使い方:
    winremap [オプション]

オプション:
    -c, --config <PATH>    設定ファイル（既定: %APPDATA%\\winremap\\config.toml）
        --lang <en|ja>     UI 言語（既定: システム言語）
        --debug            前面アプリ情報とキー判定を表示
        --macro-delay <MS> マクロの各ストローク間の待ち時間 0-15 ms（既定 0。
                           特定アプリでマクロが不安定なときは 5-10 を試す）
    -V, --version          バージョンを表示
    -h, --help             このヘルプを表示"
        ),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use winremap::keymap::{parse_input_pattern, parse_key_combo};

    // LANG is never initialized under test, so everything renders English.
    #[test]
    fn renders_notation_in_human_words() {
        let combo = parse_key_combo("C-S-h").expect("parses");
        assert_eq!(combo_human(&combo), "Ctrl + Shift + h");

        let sequence = parse_input_pattern("A-x h").expect("parses");
        assert_eq!(input_human(&sequence), "Alt + x, then h (two strokes)");

        let single = parse_input_pattern("C-h").expect("parses");
        assert_eq!(input_human(&single), "Ctrl + h");

        let steps = [
            parse_key_combo("C-Right").expect("parses"),
            parse_key_combo("C-Left").expect("parses"),
        ];
        assert_eq!(output_human(&steps), "Ctrl + Right → Ctrl + Left (macro)");
        assert_eq!(output_human(&steps[..1]), "Ctrl + Right");
    }
}
