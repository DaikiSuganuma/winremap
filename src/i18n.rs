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

use winremap::keymap::{KeyCombo, vk_display_name};

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
    pub config_window_file: &'static str,
    pub config_window_open_in_editor: &'static str,
    pub config_window_readonly: &'static str,
    pub config_window_path: &'static str,
    pub config_window_file_time: &'static str,
    /// Header of the label column in a two-column "item / value" table.
    pub config_column_field: &'static str,
    pub config_window_loaded_at: &'static str,
    pub config_unknown: &'static str,
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
    /// Marks a line the user caused, so actions stand out from [debug] noise.
    pub log_action_prefix: &'static str,
    /// Why an in-progress macro recording was dropped (design doc §5.6).
    pub macro_record_reason_reload: &'static str,
    pub macro_record_reason_disabled: &'static str,
    /// Stands in for the app name when the foreground window cannot be
    /// identified (an elevated window denies the query under UIPI).
    pub macro_record_unknown_app: &'static str,
    pub log_window_title: &'static str,
    pub log_window_hint: &'static str,
    pub log_window_follow: &'static str,
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
    config_window_file: "Config file",
    config_window_open_in_editor: "Open in text editor",
    config_window_readonly: "Showing the config in effect right now. Editing arrives in a later build — for now, edit the file and reload.",
    config_window_path: "Path",
    config_window_file_time: "File modified",
    config_column_field: "Item",
    config_window_loaded_at: "Loaded at",
    config_unknown: "unknown",
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
    config_field_exclude: "Excluded",
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
    config_help_link: "Open the help page",
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
    macro_record_reason_reload: "config reloaded",
    macro_record_reason_disabled: "remapping disabled",
    macro_record_unknown_app: "an unknown app",
    log_window_title: "WinRemap — log",
    log_window_hint: "Debug logging is on while this window is open. Press keys to see how they are handled.",
    log_window_follow: "Follow newest",
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
    config_window_file: "設定ファイル",
    config_window_open_in_editor: "テキストエディタで開く",
    config_window_readonly: "現在動作中の設定を表示しています。編集機能は今後のバージョンで追加します。設定を変更するには、ファイルを編集して再読み込みしてください。",
    config_window_path: "パス",
    config_window_file_time: "ファイル更新日時",
    config_column_field: "項目",
    config_window_loaded_at: "読み込み",
    config_unknown: "不明",
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
    config_field_exclude: "除外",
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
    config_help_link: "ヘルプページを開く",
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
    macro_record_reason_reload: "設定をリロードしたため",
    macro_record_reason_disabled: "リマップを無効にしたため",
    macro_record_unknown_app: "不明なアプリ",
    log_window_title: "WinRemap — ログ",
    log_window_hint: "このウィンドウを開いている間、デバッグログを記録します。キーを押すと処理内容が表示されます。",
    log_window_follow: "最新に追従",
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
pub fn help_url() -> &'static str {
    match lang() {
        Lang::En => "https://daikisuganuma.github.io/winremap/",
        Lang::Ja => "https://daikisuganuma.github.io/winremap/ja/",
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

pub fn debug_key_pass(input: KeyCombo) -> String {
    match lang() {
        Lang::En => format!("[debug] {input} → passed through"),
        Lang::Ja => format!("[debug] {input} → 素通し"),
    }
}

pub fn debug_key_chord(prev: Option<KeyCombo>, input: KeyCombo, target: KeyCombo) -> String {
    let input = fmt_input(prev, input);
    match lang() {
        Lang::En => format!("[debug] {input} → remapped to {target}"),
        Lang::Ja => format!("[debug] {input} → {target} に置換"),
    }
}

pub fn debug_key_substituted(input: KeyCombo, target_vk: u16) -> String {
    let target = vk_display_name(target_vk);
    match lang() {
        Lang::En => format!("[debug] {input} → substituted with {target} (bare-key rule)"),
        Lang::Ja => format!("[debug] {input} → {target} に差し替え（単キールール）"),
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
        Lang::En => format!("[debug] {input} → macro executed ({strokes} strokes: {steps})"),
        Lang::Ja => format!("[debug] {input} → マクロ実行（{strokes} ストローク: {steps}）"),
    }
}

pub fn debug_key_repeat(input: KeyCombo) -> String {
    match lang() {
        Lang::En => format!("[debug] {input} → auto-repeat (suppressed)"),
        Lang::Ja => format!("[debug] {input} → キーリピート（抑止）"),
    }
}

/// Echo of an injected event passing through the hook. `source` is one of
/// the pre-localized `debug_source_*` labels.
pub fn debug_injected(vk: u16, up: bool, source: &str) -> String {
    let key = vk_display_name(vk);
    let arrow = if up { "↑" } else { "↓" };
    match lang() {
        Lang::En => format!("[debug]   injected ({source}): {key} {arrow}"),
        Lang::Ja => format!("[debug]   注入（{source}）: {key} {arrow}"),
    }
}

pub fn debug_key_prefix(input: KeyCombo) -> String {
    match lang() {
        Lang::En => format!("[debug] {input} → prefix armed (waiting for the next key)"),
        Lang::Ja => format!("[debug] {input} → プレフィックス待機（次のキーで確定）"),
    }
}

pub fn debug_key_swallowed(prev: Option<KeyCombo>, input: KeyCombo) -> String {
    let input = fmt_input(prev, input);
    match lang() {
        Lang::En => format!("[debug] {input} → undefined sequence (swallowed)"),
        Lang::Ja => format!("[debug] {input} → 未定義のシーケンス（握りつぶし）"),
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
