//! Between `toml_edit` values and the strings the draft holds: reading a
//! value out as the text a person would type, and writing text back as a
//! value with the file's spelling conventions (quoted rule keys, arrays for
//! macros).

use toml_edit::{Array, DocumentMut, Item, Key, Table, Value};

/// A scalar as the user would type it into an edit box. Typed accessors, not
/// `Display`: the latter would drag the value's decor — indentation and the
/// trailing comment — into the editable text.
pub(super) fn scalar_text(item: &Item) -> String {
    match item.as_value() {
        Some(Value::String(s)) => s.value().clone(),
        Some(Value::Integer(i)) => i.value().to_string(),
        Some(Value::Float(f)) => f.value().to_string(),
        Some(Value::Boolean(b)) => b.value().to_string(),
        _ => String::new(),
    }
}

/// A rule's right-hand side: a chord string, or an array joined the way the
/// user would type a macro.
pub(super) fn output_text(item: &Item) -> String {
    if let Some(text) = item.as_str() {
        return text.to_owned();
    }
    if let Some(array) = item.as_array() {
        return array
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ");
    }
    scalar_text(item)
}

pub(super) fn string_list(item: Option<&Item>) -> Vec<String> {
    item.and_then(Item::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn comma_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A rule key as the file writes them: quoted. `toml_edit` renders a key it
/// was handed as a string with its default encoder, which leaves `C-n` bare —
/// legal TOML, but a stray bare key among quoted ones (owner feedback
/// 2026-07-26). Parsing the quoted form is what attaches that spelling to the
/// key; the fallback cannot trigger for key notation, which has neither
/// quotes nor backslashes to escape.
pub(super) fn rule_key(input: &str) -> Key {
    let escaped = input.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
        .parse::<Key>()
        .unwrap_or_else(|_| Key::new(input))
}

/// The comment lines standing above a deleted entry, kept so they can move to
/// the next survivor. Deleting a rule takes the rule's line — and the comment
/// written *on* that line — with it, but the lines above it are as likely to
/// introduce the group as the one rule: a `# --- Editing ---` header used to
/// vanish with the first rule under it (owner feedback 2026-07-26, ADR 0054).
///
/// Blank lines between the comments are kept, the ones around the block are
/// not: the block is being moved, and the spacing of the entry it lands on
/// takes over from there.
pub(super) fn detached_comments(prefix: &str) -> String {
    let lines: Vec<&str> = prefix.lines().collect();
    let is_comment = |line: &&str| line.trim_start().starts_with('#');
    let Some(first) = lines.iter().position(is_comment) else {
        return String::new();
    };
    let last = lines.iter().rposition(is_comment).unwrap_or(first);
    let mut kept = String::new();
    for line in &lines[first..=last] {
        // Leading, not trailing: the newline before the first comment is what
        // ends the line the deleted entry used to sit after.
        kept.push('\n');
        kept.push_str(line);
    }
    kept.push('\n');
    kept
}

pub(super) fn decor_text(raw: Option<&toml_edit::RawString>) -> String {
    raw.and_then(|raw| raw.as_str())
        .unwrap_or_default()
        .to_owned()
}

/// A rule output as a TOML value: a comma means a macro, hence an array.
pub(super) fn output_value(text: &str) -> Value {
    if text.contains(',') {
        let mut array = Array::new();
        for part in text
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            array.push(part);
        }
        Value::Array(array)
    } else {
        Value::from(text.trim())
    }
}

/// A number as typed, or the text itself when it is not one — validation
/// will point at it, which beats silently dropping what the user entered.
pub(super) fn number_value(text: &str) -> Value {
    text.trim()
        .parse::<i64>()
        .map(Value::from)
        .unwrap_or_else(|_| Value::from(text))
}

/// Replaces a key's value while keeping the spacing and trailing comment
/// around the old one; inserts at the end when the key is new.
pub(super) fn set_scalar(table: &mut Table, key: &str, mut value: Value) {
    if let Some((_, item)) = table.get_key_value_mut(key) {
        if let Some(old) = item.as_value() {
            *value.decor_mut() = old.decor().clone();
        }
        *item = Item::Value(value);
    } else {
        table.insert(key, Item::Value(value));
    }
}

pub(super) fn ensure_table<'a>(doc: &'a mut DocumentMut, key: &str) -> &'a mut Table {
    let root = doc.as_table_mut();
    if !matches!(root.get(key), Some(Item::Table(_))) {
        let mut table = Table::new();
        table.set_implicit(false);
        root.insert(key, Item::Table(table));
    }
    root.get_mut(key)
        .and_then(Item::as_table_mut)
        .expect("the table was just inserted")
}
