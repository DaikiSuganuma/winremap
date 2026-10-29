//! The command line: `--help`, bad arguments, and the notice a test build
//! carries (ADR 0053).

use std::path::Path;

use super::{Lang, lang};

/// Only for a file named by `--config`. A missing *default* config is not an
/// error any more — it is written (see [`created_default_config`]) — but a
/// path the user typed is more likely a typo than a request to create it.
pub fn no_config_file(path: &Path) -> String {
    match lang() {
        Lang::En => format!(
            "no config file at {}.\nCheck the path given to --config, or omit the flag to use the default config.",
            path.display()
        ),
        Lang::Ja => format!(
            "設定ファイルがありません: {}\n--config に渡したパスを確認するか、--config を省略して既定の設定ファイルを使ってください。",
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
    -c, --config <PATH>    Config file (default: the one last chosen in the
                           settings window, else
                           %APPDATA%\\winremap\\config.toml)
        --lang <en|ja>     UI language (default: system language)
        --debug            Print the log to this terminal (the same lines the
                           tray's \"Show log\" window shows)
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
    -c, --config <PATH>    設定ファイル（既定: 設定ウィンドウで最後に選んだ
                           ファイル。無ければ
                           %APPDATA%\\winremap\\config.toml）
        --lang <en|ja>     UI 言語（既定: システム言語）
        --debug            ログをこの端末に表示（トレイの「ログを表示」
                           ウィンドウと同じ内容）
        --macro-delay <MS> マクロの各ストローク間の待ち時間 0-15 ms（既定 0。
                           特定アプリでマクロが不安定なときは 5-10 を試す）
    -V, --version          バージョンを表示
    -h, --help             このヘルプを表示"
        ),
    }
}
