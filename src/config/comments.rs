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

use crate::keymap::{InputPattern, Layout, combo_notation, parse_input_pattern};

/// Comments belonging to one `[[keymap]]` section, in file order.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct KeymapComments {
    /// Keyed by field name (`name`, `application`, `exclude`).
    pub fields: HashMap<String, String>,
    /// Per entry of `application` / `exclude`, keyed by the lowercased exe
    /// name — a multi-line array carries a comment per line, and that is
    /// where users explain why an app is on the list.
    pub apps: HashMap<String, String>,
    pub excludes: HashMap<String, String>,
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

    /// Exe names are matched case-insensitively everywhere else, so their
    /// comments are looked up the same way.
    pub fn app(&self, exe: &str) -> Option<&str> {
        self.apps.get(&exe.to_ascii_lowercase()).map(String::as_str)
    }

    pub fn exclude(&self, exe: &str) -> Option<&str> {
        self.excludes
            .get(&exe.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// Reads a config file for its comments. Any failure yields no comments:
/// the settings window still works, it just shows none.
pub fn read(path: &Path, layout: &Layout) -> ConfigComments {
    std::fs::read_to_string(path)
        .map(|source| parse(&source, layout))
        .unwrap_or_default()
}

pub fn parse(source: &str, layout: &Layout) -> ConfigComments {
    let Ok(doc) = source.parse::<DocumentMut>() else {
        return ConfigComments::default();
    };
    let mut comments = ConfigComments::default();

    if let Some(text) = trailing_comment(doc.get("macro_delay_ms")) {
        comments.general.insert("macro_delay_ms".to_owned(), text);
    }
    if let Some(section) = doc.get("macro").and_then(Item::as_table) {
        for (key, item) in section.iter() {
            if let Some(text) = trailing_comment(Some(item)) {
                comments.general.insert(format!("macro.{key}"), text);
            }
        }
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
            comments.keymaps.push(keymap_comments(table, layout));
        }
    }
    comments
}

fn keymap_comments(table: &Table, layout: &Layout) -> KeymapComments {
    let mut comments = KeymapComments::default();
    for key in ["name", "application", "exclude"] {
        if let Some(text) = trailing_comment(table.get(key)) {
            comments.fields.insert(key.to_owned(), text);
        }
    }
    comments.apps = array_comments(table.get("application"));
    comments.excludes = array_comments(table.get("exclude"));
    if let Some(remap) = table.get("remap").and_then(Item::as_table) {
        for (key, item) in remap.iter() {
            let Some(text) = trailing_comment(Some(item)) else {
                continue;
            };
            if let Some(canonical) = canonical_input(key, layout) {
                comments.rules.insert(canonical, text);
            }
        }
    }
    comments
}

/// Trailing comments on the entries of a string array, keyed by the
/// lowercased entry. Written per line, which is the only way a long
/// `exclude` list stays explainable.
///
/// The comma sits between the entry and its comment, so `toml_edit` files
/// that comment under the *next* entry's prefix — or, for the last entry,
/// under the array's own trailing decor. Hence the look-ahead.
fn array_comments(item: Option<&Item>) -> HashMap<String, String> {
    let mut found = HashMap::new();
    let Some(array) = item.and_then(Item::as_array) else {
        return found;
    };
    for (index, value) in array.iter().enumerate() {
        let Some(name) = value.as_str() else { continue };
        let after = match array.get(index + 1) {
            Some(next) => comment_in(next.decor().prefix()),
            None => comment_in(Some(array.trailing())),
        };
        let comment = comment_in(value.decor().suffix()).or(after);
        if let Some(text) = comment {
            found.insert(name.to_ascii_lowercase(), text);
        }
    }
    found
}

/// The rule's input as the settings window renders it, so a comment can be
/// looked up by what is on screen rather than by what was typed. Public for
/// the edit mode, which holds raw spellings (ADR 0049) but still wants to
/// show the comments written next to them.
pub fn canonical_input(written: &str, layout: &Layout) -> Option<String> {
    match parse_input_pattern(written, layout).ok()? {
        InputPattern::Single(combo) => Some(combo_notation(&combo, layout)),
        InputPattern::Sequence(first, second) => Some(format!(
            "{} {}",
            combo_notation(&first, layout),
            combo_notation(&second, layout)
        )),
    }
}

/// The `# ...` that follows a value on its own line. Comments on their own
/// line land in the *next* item's prefix decor, so only suffix decor is read;
/// that is exactly the "same line" the owner asked for.
fn trailing_comment(item: Option<&Item>) -> Option<String> {
    comment_in(item?.as_value()?.decor().suffix())
}

/// The `# ...` on the same line as whatever this decor follows. A `#` after
/// a newline started a line of its own and belongs to nothing, so it is
/// dropped — "same line" is the whole point (owner decision 2026-07-21).
fn comment_in(decor: Option<&toml_edit::RawString>) -> Option<String> {
    let text = decor?.as_str()?;
    let same_line = text.lines().next().unwrap_or_default();
    let (_, comment) = same_line.split_once('#')?;
    let comment = comment.trim();
    (!comment.is_empty()).then(|| comment.to_owned())
}
