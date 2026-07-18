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
}

static EN: Texts = Texts {
    menu_enabled: "Enabled",
    menu_reload: "Reload config",
    menu_open: "Open config file",
    menu_quit: "Quit",
    tooltip_disabled: "winremap (disabled)",
    tooltip_reload_failed: "winremap — config reload FAILED (see console)",
    remapping_active: "remapping active. Use the tray icon to reload or quit.",
    already_running: "winremap is already running (check the task tray)",
    debug_none: "(none)",
    debug_foreground_unknown: "[debug] foreground: could not determine (possibly an elevated window)",
};

static JA: Texts = Texts {
    menu_enabled: "有効",
    menu_reload: "設定を再読み込み",
    menu_open: "設定ファイルを開く",
    menu_quit: "終了",
    tooltip_disabled: "winremap（無効）",
    tooltip_reload_failed: "winremap — 設定の再読み込みに失敗（コンソール参照）",
    remapping_active: "リマップ稼働中。再読み込み・終了はトレイアイコンから。",
    already_running: "winremap は既に起動しています（タスクトレイを確認してください）",
    debug_none: "（なし）",
    debug_foreground_unknown: "[debug] 前面アプリ: 取得できませんでした（管理者権限ウィンドウの可能性）",
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
            "winremap {version}: {count} keymap(s) loaded from {}",
            path.display()
        ),
        Lang::Ja => format!(
            "winremap {version}: {} からキーマップを {count} 件読み込みました",
            path.display()
        ),
    }
}

pub fn tooltip_status(count: usize) -> String {
    match lang() {
        Lang::En => format!("winremap — {count} keymap(s)"),
        Lang::Ja => format!("winremap — キーマップ {count} 件"),
    }
}

pub fn reload_ok(count: usize) -> String {
    match lang() {
        Lang::En => format!("config reloaded: {count} keymap(s)"),
        Lang::Ja => format!("設定を再読み込みしました（キーマップ {count} 件）"),
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
            "winremap {version} — per-application key remapper for Windows

USAGE:
    winremap [OPTIONS]

OPTIONS:
    -c, --config <PATH>    Config file (default: %APPDATA%\\winremap\\config.toml)
        --lang <en|ja>     UI language (default: system language)
        --debug            Print foreground-app info useful for writing the config
    -V, --version          Print version
    -h, --help             Print this help"
        ),
        Lang::Ja => format!(
            "winremap {version} — Windows 用アプリ別キーリマッパー

使い方:
    winremap [オプション]

オプション:
    -c, --config <PATH>    設定ファイル（既定: %APPDATA%\\winremap\\config.toml）
        --lang <en|ja>     UI 言語（既定: システム言語）
        --debug            設定記述に役立つ前面アプリ情報を表示
    -V, --version          バージョンを表示
    -h, --help             このヘルプを表示"
        ),
    }
}
