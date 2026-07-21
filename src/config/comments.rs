//! Trailing comments from the config file, for the settings window.
//!
//! The compiled [`crate::keymap::RemapTable`] has no idea the file had
//! comments in it, but a list of forty rules is unreadable without the notes
//! the user wrote next to them. So the file is read a second time with
//! `toml_edit`, which preserves formatting (ADR 0036), and only the trailing
//! `# ...` on each line is kept.
//!
//! Rule keys are canonicalized through the same parser the compiler uses, so
//! a comment written next to `"S-C-h"` still finds the rule the table stores
//! as `C-S-h`. Anything that fails to parse is dropped: this is presentation
//! only, and validation already reported it.

use std::collections::HashMap;
use std::path::Path;

use toml_edit::{DocumentMut, Item, Table};

use crate::keymap::{InputPattern, parse_input_pattern};

/// Comments belonging to one `[[keymap]]` section, in file order.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct KeymapComments {
    /// Keyed by field name (`name`, `application`, `exclude`).
    pub fields: HashMap<String, String>,
    /// Keyed by the canonical rendering of the rule's input pattern.
    pub rules: HashMap<String, String>,
}

/// Trailing comments for a whole config file.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ConfigComments {
    /// Top-level keys (`macro_delay_ms`) and `[ime_indicator]` fields, the
    /// latter prefixed: `ime_indicator.enabled`.
    pub general: HashMap<String, String>,
    /// One entry per `[[keymap]]`, in the order they appear in the file —
    /// the same order the compiled table keeps them in.
    pub keymaps: Vec<KeymapComments>,
}

impl ConfigComments {
    pub fn general(&self, key: &str) -> Option<&str> {
        self.general.get(key).map(String::as_str)
    }

    pub fn keymap(&self, index: usize) -> Option<&KeymapComments> {
        self.keymaps.get(index)
    }
}

impl KeymapComments {
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }

    pub fn rule(&self, canonical: &str) -> Option<&str> {
        self.rules.get(canonical).map(String::as_str)
    }
}

/// Reads a config file for its comments. Any failure yields no comments:
/// the settings window still works, it just shows none.
pub fn read(path: &Path) -> ConfigComments {
    std::fs::read_to_string(path)
        .map(|source| parse(&source))
        .unwrap_or_default()
}

pub fn parse(source: &str) -> ConfigComments {
    let Ok(doc) = source.parse::<DocumentMut>() else {
        return ConfigComments::default();
    };
    let mut comments = ConfigComments::default();

    if let Some(text) = trailing_comment(doc.get("macro_delay_ms")) {
        comments.general.insert("macro_delay_ms".to_owned(), text);
    }
    if let Some(section) = doc.get("ime_indicator").and_then(Item::as_table) {
        for (key, item) in section.iter() {
            if let Some(text) = trailing_comment(Some(item)) {
                comments
                    .general
                    .insert(format!("ime_indicator.{key}"), text);
            }
        }
    }

    if let Some(keymaps) = doc.get("keymap").and_then(Item::as_array_of_tables) {
        for table in keymaps.iter() {
            comments.keymaps.push(keymap_comments(table));
        }
    }
    comments
}

fn keymap_comments(table: &Table) -> KeymapComments {
    let mut comments = KeymapComments::default();
    for key in ["name", "application", "exclude"] {
        if let Some(text) = trailing_comment(table.get(key)) {
            comments.fields.insert(key.to_owned(), text);
        }
    }
    if let Some(remap) = table.get("remap").and_then(Item::as_table) {
        for (key, item) in remap.iter() {
            let Some(text) = trailing_comment(Some(item)) else {
                continue;
            };
            if let Some(canonical) = canonical_input(key) {
                comments.rules.insert(canonical, text);
            }
        }
    }
    comments
}

/// The rule's input as the settings window renders it, so a comment can be
/// looked up by what is on screen rather than by what was typed.
fn canonical_input(written: &str) -> Option<String> {
    match parse_input_pattern(written).ok()? {
        InputPattern::Single(combo) => Some(combo.to_string()),
        InputPattern::Sequence(first, second) => Some(format!("{first} {second}")),
    }
}

/// The `# ...` that follows a value on its own line. Comments on their own
/// line land in the *next* item's prefix decor, so only suffix decor is read;
/// that is exactly the "same line" the owner asked for.
fn trailing_comment(item: Option<&Item>) -> Option<String> {
    let suffix = item?.as_value()?.decor().suffix()?.as_str()?;
    let (_, comment) = suffix.split_once('#')?;
    // A suffix can only hold one line's comment, but trim defensively so a
    // stray newline never widens a row in the table.
    let comment = comment.lines().next().unwrap_or_default().trim();
    (!comment.is_empty()).then(|| comment.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"
macro_delay_ms = 8  # WinUI メモ帳向け

# この行は前置コメントなので拾わない
[ime_indicator]
enabled = true # IME の状態を表示

[[keymap]]
name = "emacs"  # Emacs 風
application = ["chrome.exe", "code.exe"]   #  ブラウザとエディタ
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
        assert_eq!(comments.general("macro_delay_ms"), Some("WinUI メモ帳向け"));
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
}
