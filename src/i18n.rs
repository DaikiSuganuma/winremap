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
    pub menu_enabled: &'static str,
    pub menu_reload: &'static str,
    pub menu_open: &'static str,
    pub menu_quit: &'static str,
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
    menu_enabled: "Enabled",
    menu_reload: "Reload config",
    menu_open: "Open config file",
    menu_quit: "Quit",
    tooltip_disabled: "WinRemap (disabled)",
    tooltip_reload_failed: "WinRemap — config reload FAILED (see console)",
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
    menu_enabled: "有効",
    menu_reload: "設定を再読み込み",
    menu_open: "設定ファイルを開く",
    menu_quit: "終了",
    tooltip_disabled: "WinRemap（無効）",
    tooltip_reload_failed: "WinRemap — 設定の再読み込みに失敗（コンソール参照）",
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

pub fn tooltip_status(count: usize) -> String {
    match lang() {
        Lang::En => format!("WinRemap — {count} keymap(s)"),
        Lang::Ja => format!("WinRemap — キーマップ {count} 件"),
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

/// Console message for a failed reload; `error` stays in English on purpose
/// (diagnostics policy above).
pub fn reload_failed(error: &str) -> String {
    match lang() {
        Lang::En => format!("config reload failed, keeping previous config:\n{error}"),
        Lang::Ja => format!("設定の再読み込みに失敗しました。直前の設定を維持します:\n{error}"),
    }
}

/// Indicator-thread debug line: one query outcome and what was done.
/// `via_core_window` marks queries answered through a UWP CoreWindow child
/// (ADR 0023) so UWP detection issues are visible in reports.
pub fn debug_ime_query(open: Option<bool>, shown: bool, via_core_window: bool) -> String {
    match lang() {
        Lang::En => {
            let state = match open {
                Some(true) => "ON",
                Some(false) => "OFF",
                None => "unknown",
            };
            let action = if shown { "panel shown" } else { "no panel" };
            let via = if via_core_window {
                " (via CoreWindow)"
            } else {
                ""
            };
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
            let via = if via_core_window {
                "（CoreWindow 経由）"
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
