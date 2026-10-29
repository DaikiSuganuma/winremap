//! Saving: applies the draft's changes to the file's own text (ADR 0036,
//! ADR 0049 — what the user did not edit stays byte for byte).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use toml_edit::{Array, DocumentMut, Item, Key, Table, Value};

use super::rebuild::rebuild_keymaps;
use super::toml::{
    comma_list, decor_text, detached_comments, ensure_table, number_value, output_value, rule_key,
    set_scalar,
};
use super::{ConfigDraft, KeymapDraft};

/// Applies `edited`'s differences from `original` to `source` and returns
/// the new file text. `source` is read *at save time* (design doc §4.1), so
/// `original` must be the draft `source`'s file produced when editing began —
/// the diff is what makes untouched formatting untouchable.
pub fn apply(
    source: &str,
    original: &ConfigDraft,
    edited: &ConfigDraft,
) -> Result<String, toml_edit::TomlError> {
    let mut doc: DocumentMut = source.parse()?;
    apply_general(&mut doc, original, edited);
    apply_keymaps(&mut doc, original, edited);
    Ok(keep_line_endings(source, doc.to_string()))
}

/// `toml_edit` ends every line it encodes with `\n`, so a file written on
/// Windows came back with all of its line endings changed — a one-word edit
/// produced a whole-file diff, which is the opposite of what ADR 0036
/// promises (owner acceptance 2026-07-26).
///
/// Multi-line strings are the one place a `\n` is content rather than layout,
/// and this cannot tell the two apart; a file containing one is left alone.
/// Nothing in the config schema (config-spec §3) is written that way.
fn keep_line_endings(source: &str, encoded: String) -> String {
    if !source.contains("\r\n") || source.contains("\"\"\"") || source.contains("'''") {
        return encoded;
    }
    encoded.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// Writes atomically: a sibling temp file, then a rename over the target.
/// A crash mid-write leaves the old config intact (design doc §4.4).
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        // The rename failing leaves the temp file around; removing it is
        // best-effort cleanup, the reported error stays the rename's.
        let _ = std::fs::remove_file(&tmp);
    })
}

fn apply_general(doc: &mut DocumentMut, original: &ConfigDraft, edited: &ConfigDraft) {
    if edited.macro_delay != original.macro_delay {
        if original.uses_legacy_delay_key {
            // Keep the file's own spelling (ADR 0039).
            if edited.macro_delay.is_empty() {
                doc.as_table_mut().remove("macro_delay_ms");
            } else {
                set_scalar(
                    doc.as_table_mut(),
                    "macro_delay_ms",
                    number_value(&edited.macro_delay),
                );
            }
        } else if edited.macro_delay.is_empty() {
            if let Some(section) = doc.get_mut("macro").and_then(Item::as_table_mut) {
                section.remove("delay_ms");
            }
        } else {
            set_scalar(
                ensure_table(doc, "macro"),
                "delay_ms",
                number_value(&edited.macro_delay),
            );
        }
    }

    let records = [
        (
            "record_start",
            &original.macro_record.start,
            &edited.macro_record.start,
        ),
        (
            "record_stop",
            &original.macro_record.stop,
            &edited.macro_record.stop,
        ),
        (
            "record_play",
            &original.macro_record.play,
            &edited.macro_record.play,
        ),
    ];
    for (key, orig, new) in records {
        if orig == new {
            continue;
        }
        if new.is_empty() {
            if let Some(section) = doc.get_mut("macro").and_then(Item::as_table_mut) {
                section.remove(key);
            }
        } else {
            set_scalar(ensure_table(doc, "macro"), key, Value::from(new.trim()));
        }
    }

    let bools = [
        ("enabled", original.ime.enabled, edited.ime.enabled),
        (
            "show_app_name",
            original.ime.show_app_name,
            edited.ime.show_app_name,
        ),
        (
            "change_cursor_color",
            original.ime.change_cursor_color,
            edited.ime.change_cursor_color,
        ),
    ];
    for (key, orig, new) in bools {
        if orig == new {
            continue;
        }
        match new {
            Some(value) => set_scalar(ensure_table(doc, "ime_indicator"), key, Value::from(value)),
            None => {
                if let Some(section) = doc.get_mut("ime_indicator").and_then(Item::as_table_mut) {
                    section.remove(key);
                }
            }
        }
    }

    let numbers = [
        (
            "duration_ms",
            &original.ime.duration_ms,
            &edited.ime.duration_ms,
        ),
        ("size", &original.ime.size, &edited.ime.size),
        ("opacity", &original.ime.opacity, &edited.ime.opacity),
    ];
    for (key, orig, new) in numbers {
        if orig == new {
            continue;
        }
        if new.is_empty() {
            if let Some(section) = doc.get_mut("ime_indicator").and_then(Item::as_table_mut) {
                section.remove(key);
            }
        } else {
            set_scalar(ensure_table(doc, "ime_indicator"), key, number_value(new));
        }
    }

    if edited.ime.cursor_color != original.ime.cursor_color {
        let new = edited.ime.cursor_color.trim();
        if new.is_empty() {
            if let Some(section) = doc.get_mut("ime_indicator").and_then(Item::as_table_mut) {
                section.remove("cursor_color");
            }
        } else {
            set_scalar(
                ensure_table(doc, "ime_indicator"),
                "cursor_color",
                Value::from(new),
            );
        }
    }

    if edited.ime.trigger_keys != original.ime.trigger_keys {
        let list = comma_list(&edited.ime.trigger_keys);
        if list.is_empty() {
            if let Some(section) = doc.get_mut("ime_indicator").and_then(Item::as_table_mut) {
                section.remove("trigger_keys");
            }
        } else {
            rewrite_string_array(
                ensure_table(doc, "ime_indicator"),
                "trigger_keys",
                &comma_list(&original.ime.trigger_keys),
                &list,
            );
        }
    }
}

fn apply_keymaps(doc: &mut DocumentMut, original: &ConfigDraft, edited: &ConfigDraft) {
    // A draft read from a file numbers its keymaps 0..n, so "same members in
    // the same order" reads directly off the origins.
    let structure_unchanged = edited.keymaps.len() == original.keymaps.len()
        && edited
            .keymaps
            .iter()
            .enumerate()
            .all(|(index, keymap)| keymap.origin == Some(index));

    if structure_unchanged {
        if let Some(keymaps) = doc
            .as_table_mut()
            .get_mut("keymap")
            .and_then(Item::as_array_of_tables_mut)
        {
            for (index, table) in keymaps.iter_mut().enumerate() {
                apply_keymap_fields(table, &original.keymaps[index], &edited.keymaps[index]);
            }
        }
        return;
    }
    rebuild_keymaps(doc, original, edited);
}

pub(super) fn apply_keymap_fields(table: &mut Table, original: &KeymapDraft, edited: &KeymapDraft) {
    if edited.name != original.name {
        if edited.name.is_empty() {
            table.remove("name");
        } else {
            set_scalar(table, "name", Value::from(edited.name.clone()));
        }
    }
    rewrite_string_array(
        table,
        "application",
        &original.application,
        &edited.application,
    );
    rewrite_string_array(table, "exclude", &original.exclude, &edited.exclude);
    if edited.rules != original.rules {
        rewrite_rules(table, original, edited);
    }
}

/// Replaces a string array, reusing the old elements — with their
/// indentation and per-entry comments — for every entry that survives.
fn rewrite_string_array(table: &mut Table, key: &str, original: &[String], edited: &[String]) {
    if original == edited {
        return;
    }
    let old = table.get(key).and_then(Item::as_array);
    let mut pool: Vec<Value> = old
        .map(|array| array.iter().cloned().collect())
        .unwrap_or_default();
    let mut array = Array::new();
    for entry in edited {
        match pool.iter().position(|value| value.as_str() == Some(entry)) {
            Some(index) => array.push_formatted(pool.remove(index)),
            None => array.push(entry.as_str()),
        }
    }
    if let Some(old) = old {
        array.set_trailing(old.trailing().clone());
        array.set_trailing_comma(old.trailing_comma() && !array.is_empty());
    }
    set_scalar(table, key, Value::Array(array));
}

/// Rebuilds `[keymap.remap]` in the draft's order, carrying each surviving
/// entry over whole — key formatting, attached comment, value comment. An
/// entry the user deleted takes the comment on its own line with it; the
/// comment *lines* above it move to the next surviving entry (design doc
/// §4.2). Comments stranded behind the last survivor go with the deletion —
/// there is nothing left for them to introduce.
fn rewrite_rules(table: &mut Table, original: &KeymapDraft, edited: &KeymapDraft) {
    let mut old = match table.remove("remap") {
        Some(Item::Table(remap)) => remap,
        _ => Table::new(),
    };
    let mut new = Table::new();
    *new.decor_mut() = old.decor().clone();
    new.set_implicit(old.is_implicit());
    new.set_position(old.position());

    let survives: HashSet<usize> = edited.rules.iter().filter_map(|rule| rule.origin).collect();
    let mut pool: HashMap<usize, (Key, Item)> = HashMap::new();
    let mut carry = String::new();
    for (index, rule) in original.rules.iter().enumerate() {
        let Some((mut key, item)) = old.remove_entry(&rule.input) else {
            continue;
        };
        if survives.contains(&index) {
            if !carry.is_empty() {
                let prefix = decor_text(key.leaf_decor().prefix());
                // The carry already ends the last comment line; the
                // survivor's own leading newline would leave a blank one.
                let prefix = prefix.strip_prefix('\n').unwrap_or(&prefix);
                key.leaf_decor_mut().set_prefix(format!("{carry}{prefix}"));
                carry.clear();
            }
            pool.insert(index, (key, item));
        } else {
            carry.push_str(&detached_comments(&decor_text(key.leaf_decor().prefix())));
        }
    }

    for rule in &edited.rules {
        let taken = rule
            .origin
            .and_then(|i| pool.remove(&i).map(|entry| (i, entry)));
        match taken {
            Some((index, (mut key, mut item))) => {
                let orig = &original.rules[index];
                if rule.input != orig.input {
                    let mut renamed = rule_key(&rule.input);
                    *renamed.leaf_decor_mut() = key.leaf_decor().clone();
                    key = renamed;
                }
                if rule.output != orig.output {
                    let mut value = output_value(&rule.output);
                    if let Some(old_value) = item.as_value() {
                        *value.decor_mut() = old_value.decor().clone();
                    }
                    item = Item::Value(value);
                }
                new.insert_formatted(&key, item);
            }
            None => {
                new.insert_formatted(
                    &rule_key(&rule.input),
                    Item::Value(output_value(&rule.output)),
                );
            }
        }
    }
    table.insert("remap", Item::Table(new));
}
