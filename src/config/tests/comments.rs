//! Tests for `config::comments` (moved out of the module file, guidelines §5).

use crate::config::comments::{self, ConfigComments};
use crate::keymap::tests::us_101;

/// Comments are keyed by the canonical rule spelling, which for a symbol
/// key depends on the keyboard — so the cases fix one.
fn parse(source: &str) -> ConfigComments {
    comments::parse(source, &us_101())
}

const SOURCE: &str = r#"
[macro]
delay_ms = 8  # WinUI メモ帳向け

# この行は前置コメントなので拾わない
[ime_indicator]
enabled = true # IME の状態を表示

[[keymap]]
name = "emacs"  # Emacs 風
application = [
  "Chrome.exe",  # ブラウザ
  "code.exe",
]   #  ブラウザとエディタ
exclude = []

[keymap.remap]
"C-h" = "Back"      # 削除
"S-C-h" = "Delete"  # 正規化されるはず
"A-x h" = "C-z"     # 2 ストローク
"C-t" = ["C-Right", "C-Left"]  # マクロ
"C-n" = "Down"

[[keymap]]
application = ["*"]

[keymap.remap]
"CapsLock" = "LCtrl"  # 単キー
"#;

#[test]
fn reads_general_and_ime_comments() {
    let comments = parse(SOURCE);
    assert_eq!(comments.general("macro.delay_ms"), Some("WinUI メモ帳向け"));
    assert_eq!(
        comments.general("ime_indicator.enabled"),
        Some("IME の状態を表示")
    );
}

#[test]
fn reads_keymap_field_comments() {
    let comments = parse(SOURCE);
    let first = comments.keymap(0).expect("first keymap");
    assert_eq!(first.field("name"), Some("Emacs 風"));
    assert_eq!(first.field("application"), Some("ブラウザとエディタ"));
    // No comment on that line, and none invented from elsewhere.
    assert_eq!(first.field("exclude"), None);
}

#[test]
fn reads_rule_comments_by_canonical_form() {
    let comments = parse(SOURCE);
    let first = comments.keymap(0).expect("first keymap");
    assert_eq!(first.rule("C-h"), Some("削除"));
    // Written "S-C-h", rendered "C-S-h" — the lookup has to survive that.
    assert_eq!(first.rule("C-S-h"), Some("正規化されるはず"));
    assert_eq!(first.rule("A-x h"), Some("2 ストローク"));
    assert_eq!(first.rule("C-t"), Some("マクロ"));
    assert_eq!(first.rule("C-n"), None);

    let second = comments.keymap(1).expect("second keymap");
    assert_eq!(second.rule("CapsLock"), Some("単キー"));
}

#[test]
fn reads_per_entry_comments_case_insensitively() {
    let comments = parse(SOURCE);
    let first = comments.keymap(0).expect("first keymap");
    // Written "Chrome.exe"; the table stores whatever the user typed, and
    // lookups have to work either way.
    assert_eq!(first.app("chrome.exe"), Some("ブラウザ"));
    assert_eq!(first.app("Chrome.exe"), Some("ブラウザ"));
    assert_eq!(first.app("code.exe"), None);
}

#[test]
fn keymaps_keep_file_order() {
    let comments = parse(SOURCE);
    assert_eq!(comments.keymaps.len(), 2);
    assert!(comments.keymap(0).expect("first").field("name").is_some());
    assert!(comments.keymap(1).expect("second").field("name").is_none());
}

#[test]
fn broken_toml_yields_no_comments() {
    assert_eq!(parse("this is not = = toml"), ConfigComments::default());
}

#[test]
fn own_line_comments_are_not_attributed_to_a_value() {
    let comments = parse("# lonely\nmacro_delay_ms = 4\n");
    assert_eq!(comments.general("macro_delay_ms"), None);
}
