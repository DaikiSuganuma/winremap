//! The editable draft of a config file (ADR 0049, milestone B2).
//!
//! Two models exist side by side: the *view* is the live, compiled
//! [`crate::keymap::RemapTable`] the hook resolves against, and the *edit* is
//! this draft — built from the file itself, holding the exact strings the
//! user wrote (`"S-C-h"` stays `"S-C-h"`, `"Bak"` mid-typing is fine). The
//! draft never touches the live table; saving applies the draft's *changes*
//! to a freshly read `toml_edit::DocumentMut` so everything the user did not
//! edit — comments, blank lines, ordering, spellings — survives verbatim
//! (ADR 0036).
//!
//! Rule keys the draft writes are quoted, matching how the rules around them
//! are written; `toml_edit`'s own encoder would leave `C-y` bare, which is the
//! same TOML but reads as a different kind of line (owner feedback
//! 2026-07-26).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::SystemTime;

use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Key, Table, Value};

/// A whole config file as editable strings, in file order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfigDraft {
    pub keymaps: Vec<KeymapDraft>,
    /// `[macro] delay_ms` as written; empty = the key is absent.
    pub macro_delay: String,
    pub macro_record: MacroRecordDraft,
    pub ime: ImeIndicatorDraft,
    /// The file spells the delay `macro_delay_ms` (v0.1, ADR 0039). Saving
    /// keeps whichever spelling the file already uses.
    pub uses_legacy_delay_key: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeymapDraft {
    /// Index of the `[[keymap]]` this section came from, `None` for one the
    /// user added. The diff keys off it, so reordering and renaming still
    /// find the original table with all its formatting.
    pub origin: Option<usize>,
    pub name: String,
    pub application: Vec<String>,
    pub exclude: Vec<String>,
    pub rules: Vec<RuleDraft>,
}

/// One remap rule, both sides as the raw key-notation strings. A macro
/// output is comma-separated, matching what the user would write in the
/// file's array form.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleDraft {
    /// Index into the original keymap's rule list; see [`KeymapDraft::origin`].
    pub origin: Option<usize>,
    pub input: String,
    pub output: String,
}

/// `[macro] record_*` keys as written; empty = absent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MacroRecordDraft {
    pub start: String,
    pub stop: String,
    pub play: String,
}

/// `[ime_indicator]` as written. Numbers stay strings so a half-typed value
/// survives a frame; `None` for a bool means the key is absent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImeIndicatorDraft {
    pub enabled: Option<bool>,
    pub duration_ms: String,
    pub size: String,
    pub opacity: String,
    pub show_app_name: Option<bool>,
    /// Comma-separated, like a macro output.
    pub trigger_keys: String,
    /// Independent of `enabled` (ADR 0067), so it is edited whether or not
    /// the panel is switched on.
    pub change_cursor_color: Option<bool>,
    /// `#rrggbb` as written. Kept as text for the same reason the numbers
    /// are: a half-typed colour has to survive the frame it is typed in.
    pub cursor_color: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    // toml_edit's error renders line/column plus a source snippet.
    #[error("TOML syntax error:\n{0}")]
    Toml(#[from] toml_edit::TomlError),
}

/// What the file looked like when it was read: enough to notice it changed
/// behind our back (external-change detection, design doc §6.2), nothing
/// that requires reading its contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub modified: Option<SystemTime>,
    pub len: u64,
}

pub fn stamp(path: &Path) -> std::io::Result<FileStamp> {
    let meta = std::fs::metadata(path)?;
    Ok(FileStamp {
        modified: meta.modified().ok(),
        len: meta.len(),
    })
}

/// Reads a file into a draft, with the stamp for external-change detection.
/// The only hard failure is a file that cannot be read or is not TOML at
/// all; questionable *values* stay editable and are validation's problem.
pub fn read(path: &Path) -> Result<(ConfigDraft, FileStamp), DraftError> {
    let stamp = stamp(path)?;
    let source = std::fs::read_to_string(path)?;
    Ok((parse(&source)?, stamp))
}

pub fn parse(source: &str) -> Result<ConfigDraft, toml_edit::TomlError> {
    let doc: DocumentMut = source.parse()?;
    let mut draft = ConfigDraft::default();

    if let Some(item) = doc.get("macro_delay_ms") {
        draft.uses_legacy_delay_key = true;
        draft.macro_delay = scalar_text(item);
    }
    if let Some(section) = doc.get("macro").and_then(Item::as_table) {
        if let Some(item) = section.get("delay_ms") {
            draft.macro_delay = scalar_text(item);
        }
        draft.macro_record = MacroRecordDraft {
            start: section
                .get("record_start")
                .map(scalar_text)
                .unwrap_or_default(),
            stop: section
                .get("record_stop")
                .map(scalar_text)
                .unwrap_or_default(),
            play: section
                .get("record_play")
                .map(scalar_text)
                .unwrap_or_default(),
        };
    }
    if let Some(section) = doc.get("ime_indicator").and_then(Item::as_table) {
        draft.ime = ImeIndicatorDraft {
            enabled: section.get("enabled").and_then(Item::as_bool),
            duration_ms: section
                .get("duration_ms")
                .map(scalar_text)
                .unwrap_or_default(),
            size: section.get("size").map(scalar_text).unwrap_or_default(),
            opacity: section.get("opacity").map(scalar_text).unwrap_or_default(),
            show_app_name: section.get("show_app_name").and_then(Item::as_bool),
            trigger_keys: section
                .get("trigger_keys")
                .map(output_text)
                .unwrap_or_default(),
            change_cursor_color: section.get("change_cursor_color").and_then(Item::as_bool),
            cursor_color: section
                .get("cursor_color")
                .map(scalar_text)
                .unwrap_or_default(),
        };
    }
    if let Some(keymaps) = doc.get("keymap").and_then(Item::as_array_of_tables) {
        for (index, table) in keymaps.iter().enumerate() {
            draft.keymaps.push(keymap_draft(index, table));
        }
    }
    Ok(draft)
}

fn keymap_draft(index: usize, table: &Table) -> KeymapDraft {
    let rules = table
        .get("remap")
        .and_then(Item::as_table)
        .map(|remap| {
            remap
                .iter()
                .enumerate()
                .map(|(i, (input, item))| RuleDraft {
                    origin: Some(i),
                    input: input.to_owned(),
                    output: output_text(item),
                })
                .collect()
        })
        .unwrap_or_default();
    KeymapDraft {
        origin: Some(index),
        name: table
            .get("name")
            .and_then(Item::as_str)
            .unwrap_or_default()
            .to_owned(),
        application: string_list(table.get("application")),
        exclude: string_list(table.get("exclude")),
        rules,
    }
}

/// A scalar as the user would type it into an edit box. Typed accessors, not
/// `Display`: the latter would drag the value's decor — indentation and the
/// trailing comment — into the editable text.
fn scalar_text(item: &Item) -> String {
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
fn output_text(item: &Item) -> String {
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

fn string_list(item: Option<&Item>) -> Vec<String> {
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

fn comma_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

// ---------------------------------------------------------------------------
// Saving: apply the draft's changes to the file's own text.

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

/// `toml_edit` renders tables sorted by their `position`, so adding,
/// removing or reordering `[[keymap]]` tables means handing out new
/// positions. Everything is renumbered with a stride first so the new
/// arrangement has room between the tables that are not moving.
const RENUMBER_STRIDE: isize = 100;

fn rebuild_keymaps(doc: &mut DocumentMut, original: &ConfigDraft, edited: &ConfigDraft) {
    renumber_all(doc, RENUMBER_STRIDE);

    // Pull the old tables out, addressable by original index.
    let mut old: Vec<Option<Table>> = Vec::new();
    if let Some(Item::ArrayOfTables(mut array)) = doc.as_table_mut().remove("keymap") {
        while !array.is_empty() {
            old.push(Some(array.remove(0)));
        }
    }

    // The comment block above the first `[[keymap]]` introduces the place the
    // keymaps start — in a file that opens with one, it is the file's own
    // header. `toml_edit` keeps it as that table's prefix, so moving or
    // deleting that keymap used to carry it off or destroy it (owner
    // acceptance 2026-07-26; same reasoning as ADR 0054). Detached here and
    // put back on whichever keymap ends up first.
    let lead = old.first_mut().and_then(Option::as_mut).map(|table| {
        let prefix = decor_text(table.decor().prefix());
        table.decor_mut().set_prefix("\n");
        prefix
    });

    // The keymaps stay in the stretch of the file they occupied. Sequential
    // positions from its start fit up to a stride's worth of additions
    // before the next non-keymap table — beyond any real editing session.
    let base = old
        .iter()
        .flatten()
        .filter_map(Table::position)
        .min()
        .unwrap_or_else(|| max_position(doc) + RENUMBER_STRIDE);

    let mut new = ArrayOfTables::new();
    for (index, keymap) in edited.keymaps.iter().enumerate() {
        let taken = keymap.origin.and_then(|i| {
            old.get_mut(i)
                .and_then(Option::take)
                .map(|table| (i, table))
        });
        let mut table = match taken {
            Some((i, mut table)) => {
                apply_keymap_fields(&mut table, &original.keymaps[i], keymap);
                table
            }
            None => new_keymap_table(keymap),
        };
        let position = base + index as isize;
        table.set_position(Some(position));
        // The nested [keymap.remap] shares the position; the encoder breaks
        // the tie in visit order, which is parent-then-child.
        if let Some(remap) = table.get_mut("remap").and_then(Item::as_table_mut) {
            remap.set_position(Some(position));
        }
        new.push(table);
    }
    if let Some(lead) = lead {
        match new.get_mut(0) {
            Some(first) => {
                let own = decor_text(first.decor().prefix());
                // `lead` already ends the line it sits on; the newline the
                // table carried for its own separation would leave a blank one.
                let own = own.strip_prefix('\n').unwrap_or(&own);
                first.decor_mut().set_prefix(format!("{lead}{own}"));
            }
            // No keymap is left for the block to introduce, but it is still
            // the user's text: deleting the only keymap of a file that opens
            // with one emptied the file outright (owner report 2026-07-26).
            // Keymaps come last in a config, so what follows them is where
            // the block belongs.
            None => {
                let trailing = decor_text(Some(doc.trailing()));
                let lead = lead.trim_start_matches('\n');
                doc.set_trailing(format!("{lead}{trailing}"));
            }
        }
    }
    if !new.is_empty() {
        doc.as_table_mut()
            .insert("keymap", Item::ArrayOfTables(new));
    }
}

/// Gives every table `stride`-spaced positions in its current *render*
/// order. Mirrors the encoder: tables sort by position, `None` inherits the
/// last visited one, ties keep visit order.
fn renumber_all(doc: &mut DocumentMut, stride: isize) {
    let mut effective = Vec::new();
    let mut last = 0isize;
    visit_tables(doc.as_table(), &mut |table| {
        if let Some(position) = table.position() {
            last = position;
        }
        effective.push(last);
    });

    let mut order: Vec<usize> = (0..effective.len()).collect();
    order.sort_by_key(|&index| (effective[index], index));
    let mut rank = vec![0isize; effective.len()];
    for (sorted, &index) in order.iter().enumerate() {
        rank[index] = sorted as isize * stride;
    }

    let mut index = 0;
    visit_tables_mut(doc.as_table_mut(), &mut |table| {
        table.set_position(Some(rank[index]));
        index += 1;
    });
}

/// Same traversal as `toml_edit`'s encoder: the table itself unless dotted,
/// then its children in item order. Both passes of `renumber_all` rely on
/// the order being identical.
fn visit_tables<'t>(table: &'t Table, visit: &mut impl FnMut(&'t Table)) {
    if !table.is_dotted() {
        visit(table);
    }
    for (_, item) in table.iter() {
        match item {
            Item::Table(nested) => visit_tables(nested, visit),
            Item::ArrayOfTables(array) => {
                for nested in array.iter() {
                    visit_tables(nested, visit);
                }
            }
            _ => {}
        }
    }
}

fn visit_tables_mut(table: &mut Table, visit: &mut impl FnMut(&mut Table)) {
    if !table.is_dotted() {
        visit(table);
    }
    for (_, item) in table.iter_mut() {
        match item {
            Item::Table(nested) => visit_tables_mut(nested, visit),
            Item::ArrayOfTables(array) => {
                for nested in array.iter_mut() {
                    visit_tables_mut(nested, visit);
                }
            }
            _ => {}
        }
    }
}

fn max_position(doc: &DocumentMut) -> isize {
    let mut max = 0;
    visit_tables(doc.as_table(), &mut |table| {
        if let Some(position) = table.position() {
            max = max.max(position);
        }
    });
    max
}

fn apply_keymap_fields(table: &mut Table, original: &KeymapDraft, edited: &KeymapDraft) {
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

/// A rule key as the file writes them: quoted. `toml_edit` renders a key it
/// was handed as a string with its default encoder, which leaves `C-n` bare —
/// legal TOML, but a stray bare key among quoted ones (owner feedback
/// 2026-07-26). Parsing the quoted form is what attaches that spelling to the
/// key; the fallback cannot trigger for key notation, which has neither
/// quotes nor backslashes to escape.
fn rule_key(input: &str) -> Key {
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
fn detached_comments(prefix: &str) -> String {
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

fn decor_text(raw: Option<&toml_edit::RawString>) -> String {
    raw.and_then(|raw| raw.as_str())
        .unwrap_or_default()
        .to_owned()
}

fn new_keymap_table(keymap: &KeymapDraft) -> Table {
    let mut table = Table::new();
    if !keymap.name.is_empty() {
        table.insert("name", Item::Value(Value::from(keymap.name.clone())));
    }
    let mut apps = Array::new();
    for app in &keymap.application {
        apps.push(app.as_str());
    }
    table.insert("application", Item::Value(Value::Array(apps)));
    if !keymap.exclude.is_empty() {
        let mut excludes = Array::new();
        for exe in &keymap.exclude {
            excludes.push(exe.as_str());
        }
        table.insert("exclude", Item::Value(Value::Array(excludes)));
    }
    if !keymap.rules.is_empty() {
        let mut remap = Table::new();
        for rule in &keymap.rules {
            remap.insert_formatted(
                &rule_key(&rule.input),
                Item::Value(output_value(&rule.output)),
            );
        }
        table.insert("remap", Item::Table(remap));
    }
    table
}

/// A rule output as a TOML value: a comma means a macro, hence an array.
fn output_value(text: &str) -> Value {
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
fn number_value(text: &str) -> Value {
    text.trim()
        .parse::<i64>()
        .map(Value::from)
        .unwrap_or_else(|_| Value::from(text))
}

/// Replaces a key's value while keeping the spacing and trailing comment
/// around the old one; inserts at the end when the key is new.
fn set_scalar(table: &mut Table, key: &str, mut value: Value) {
    if let Some((_, item)) = table.get_key_value_mut(key) {
        if let Some(old) = item.as_value() {
            *value.decor_mut() = old.decor().clone();
        }
        *item = Item::Value(value);
    } else {
        table.insert(key, Item::Value(value));
    }
}

fn ensure_table<'a>(doc: &'a mut DocumentMut, key: &str) -> &'a mut Table {
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

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"# 全体コメント

[macro]
delay_ms = 8  # ペース

[ime_indicator]
enabled = true
trigger_keys = ["C-Space"]  # 切替

[[keymap]]
name = "emacs"  # Emacs 風
application = [
  "Chrome.exe",  # ブラウザ
  "code.exe",
]

[keymap.remap]
"C-h" = "Back"      # 削除
# 独立コメント

# 密着コメント
"S-C-h" = "Delete"
"C-t" = ["C-Right", "C-Left"]  # マクロ

[[keymap]]
application = ["*"]
exclude = ["game.exe"]

[keymap.remap]
CapsLock = "LCtrl"
"#;

    fn draft() -> ConfigDraft {
        parse(SOURCE).expect("fixture parses")
    }

    #[test]
    fn parse_keeps_spellings_and_file_order() {
        let draft = draft();
        assert_eq!(draft.macro_delay, "8");
        assert!(!draft.uses_legacy_delay_key);
        assert_eq!(draft.ime.enabled, Some(true));
        assert_eq!(draft.ime.trigger_keys, "C-Space");
        assert_eq!(draft.keymaps.len(), 2);

        let first = &draft.keymaps[0];
        assert_eq!(first.name, "emacs");
        assert_eq!(first.application, ["Chrome.exe", "code.exe"]);
        let rules: Vec<(&str, &str)> = first
            .rules
            .iter()
            .map(|rule| (rule.input.as_str(), rule.output.as_str()))
            .collect();
        // "S-C-h" stays as written — no canonicalization (ADR 0049), and the
        // file order survives serde-free parsing.
        assert_eq!(
            rules,
            [
                ("C-h", "Back"),
                ("S-C-h", "Delete"),
                ("C-t", "C-Right, C-Left"),
            ]
        );

        let second = &draft.keymaps[1];
        assert_eq!(second.application, ["*"]);
        assert_eq!(second.exclude, ["game.exe"]);
        assert_eq!(second.rules[0].input, "CapsLock");
    }

    #[test]
    fn parse_reads_the_legacy_delay_spelling() {
        let draft = parse("macro_delay_ms = 4  # 旧綴り\n").expect("parses");
        assert!(draft.uses_legacy_delay_key);
        assert_eq!(draft.macro_delay, "4");
    }

    #[test]
    fn unchanged_draft_round_trips_identically() {
        let original = draft();
        let edited = original.clone();
        assert_eq!(apply(SOURCE, &original, &edited).expect("applies"), SOURCE);
    }

    #[test]
    fn output_change_keeps_place_and_comments() {
        let original = draft();
        let mut edited = original.clone();
        edited.keymaps[0].rules[0].output = "Delete".to_owned();
        let saved = apply(SOURCE, &original, &edited).expect("applies");
        assert!(
            saved.contains("\"C-h\" = \"Delete\"      # 削除\n"),
            "{saved}"
        );
        // Everything else untouched.
        assert!(
            saved.contains("# 密着コメント\n\"S-C-h\" = \"Delete\"\n"),
            "{saved}"
        );
        assert!(saved.contains("\"Chrome.exe\",  # ブラウザ"), "{saved}");
    }

    #[test]
    fn added_rule_lands_at_the_table_end() {
        let original = draft();
        let mut edited = original.clone();
        edited.keymaps[0].rules.push(RuleDraft {
            origin: None,
            input: "C-n".to_owned(),
            output: "Down".to_owned(),
        });
        let saved = apply(SOURCE, &original, &edited).expect("applies");
        // Quoted like the rules around it, not bare as toml_edit would write
        // it (owner feedback 2026-07-26).
        assert!(
            saved.contains("\"C-t\" = [\"C-Right\", \"C-Left\"]  # マクロ\n\"C-n\" = \"Down\"\n"),
            "{saved}"
        );
    }

    #[test]
    fn deleted_rule_keeps_the_comment_lines_above_it() {
        let original = draft();
        let mut edited = original.clone();
        // Drop "S-C-h". Both comment blocks above it stand on their own
        // lines, so both move down to the next entry; only the rule's own
        // line goes (ADR 0054).
        edited.keymaps[0].rules.remove(1);
        let saved = apply(SOURCE, &original, &edited).expect("applies");
        assert!(!saved.contains("S-C-h"), "{saved}");
        assert!(
            saved.contains(
                "# 独立コメント\n\n# 密着コメント\n\"C-t\" = [\"C-Right\", \"C-Left\"]  # マクロ"
            ),
            "{saved}"
        );
    }

    /// The report that changed the rule (owner, acceptance C-4 2026-07-26): a
    /// section header sits right on top of the first rule it introduces, so
    /// "attached to the entry" was the wrong test for what belongs to it.
    #[test]
    fn deleting_the_first_rule_of_a_group_keeps_its_header() {
        let source = "[[keymap]]\napplication = [\"*\"]\n\n[keymap.remap]\n\
                      # --- Editing ---\n\"C-h\" = \"Back\"\n\"C-d\" = \"Delete\"\n";
        let original = parse(source).expect("parses");
        let mut edited = original.clone();
        edited.keymaps[0].rules.remove(0);
        let saved = apply(source, &original, &edited).expect("applies");
        assert!(!saved.contains("C-h"), "{saved}");
        assert!(
            saved.contains("# --- Editing ---\n\"C-d\" = \"Delete\""),
            "{saved}"
        );
    }

    #[test]
    fn renamed_input_keeps_place_and_comment() {
        let original = draft();
        let mut edited = original.clone();
        edited.keymaps[0].rules[0].input = "C-y".to_owned();
        let saved = apply(SOURCE, &original, &edited).expect("applies");
        // Same slot, same trailing comment, still quoted.
        assert!(
            saved.contains("\"C-y\" = \"Back\"      # 削除\n# 独立コメント"),
            "{saved}"
        );
        assert!(!saved.contains("\"C-h\""), "{saved}");
    }

    /// Found in the acceptance run's own fixtures (2026-07-26): a file saved
    /// from the editor came back with every line ending changed, so a
    /// one-word edit showed up as a whole-file diff.
    #[test]
    fn saving_keeps_windows_line_endings() {
        let source = SOURCE.replace('\n', "\r\n");
        let original = parse(&source).expect("parses");
        assert_eq!(
            apply(&source, &original, &original.clone()).expect("applies"),
            source,
            "an untouched draft must write the file back byte for byte"
        );

        let mut edited = original.clone();
        edited.keymaps[0].rules[0].output = "Delete".to_owned();
        let saved = apply(&source, &original, &edited).expect("applies");
        assert_eq!(
            saved.matches('\n').count(),
            saved.matches("\r\n").count(),
            "no line may come back with a bare LF: {saved:?}"
        );
    }

    /// A file that opens with `[[keymap]]` keeps its header comment there —
    /// `toml_edit` files that block under the first table, so it used to
    /// travel with that keymap (or die with it). Same run as above.
    #[test]
    fn the_file_header_stays_above_the_first_keymap() {
        let source = "# file header\n# second line\n\n[[keymap]]\nname = \"a\"\n\
                      application = [\"a.exe\"]\n\n[[keymap]]\nname = \"b\"\n\
                      application = [\"b.exe\"]\n";
        let original = parse(source).expect("parses");

        let mut reordered = original.clone();
        reordered.keymaps.swap(0, 1);
        let saved = apply(source, &original, &reordered).expect("applies");
        assert!(
            saved.starts_with("# file header\n# second line\n\n[[keymap]]\nname = \"b\""),
            "{saved}"
        );

        let mut without_first = original.clone();
        without_first.keymaps.remove(0);
        let saved = apply(source, &original, &without_first).expect("applies");
        assert!(
            saved.starts_with("# file header\n# second line\n\n[[keymap]]\nname = \"b\""),
            "{saved}"
        );
    }

    /// Deleting the *only* keymap left nothing for the header to sit above,
    /// and the whole file — comments included — went with it (owner report
    /// 2026-07-26, on `examples/minimal.toml`).
    #[test]
    fn deleting_the_last_keymap_still_keeps_the_file_header() {
        let source = "# file header\n# second line\n\n[[keymap]]\nname = \"a\"\n\
                      application = [\"a.exe\"]\n\n[keymap.remap]\n\"C-h\" = \"Back\"\n";
        let original = parse(source).expect("parses");
        let mut edited = original.clone();
        edited.keymaps.clear();
        let saved = apply(source, &original, &edited).expect("applies");
        assert!(
            saved.starts_with("# file header\n# second line\n"),
            "{saved}"
        );
        assert!(!saved.contains("[[keymap]]"), "{saved}");
    }

    #[test]
    fn reordered_keymaps_move_whole_tables() {
        let original = draft();
        let mut edited = original.clone();
        edited.keymaps.swap(0, 1);
        let saved = apply(SOURCE, &original, &edited).expect("applies");
        let star = saved.find("application = [\"*\"]").expect("global keymap");
        let emacs = saved.find("name = \"emacs\"").expect("named keymap");
        assert!(star < emacs, "{saved}");
        // Each keymap kept its own remap table.
        let caps = saved.find("CapsLock").expect("global rule");
        assert!(caps < emacs, "{saved}");
        // Comments travelled with their lines.
        assert!(
            saved.contains("# 密着コメント\n\"S-C-h\" = \"Delete\""),
            "{saved}"
        );
        // The general sections stayed put at the top.
        assert!(saved.find("[macro]").expect("macro") < star, "{saved}");
    }

    #[test]
    fn added_keymap_appends_a_full_section() {
        let original = draft();
        let mut edited = original.clone();
        edited.keymaps.push(KeymapDraft {
            origin: None,
            name: "notepad".to_owned(),
            application: vec!["notepad.exe".to_owned()],
            exclude: Vec::new(),
            rules: vec![RuleDraft {
                origin: None,
                input: "C-s".to_owned(),
                output: "C-w".to_owned(),
            }],
        });
        let saved = apply(SOURCE, &original, &edited).expect("applies");
        assert!(saved.contains("name = \"notepad\""), "{saved}");
        assert!(saved.contains("application = [\"notepad.exe\"]"), "{saved}");
        assert!(saved.contains("\"C-s\" = \"C-w\""), "{saved}");
        assert!(
            saved.find("notepad").expect("new section") > saved.find("CapsLock").expect("old rule"),
            "{saved}"
        );
    }

    #[test]
    fn deleted_keymap_disappears_with_its_rules() {
        let original = draft();
        let mut edited = original.clone();
        edited.keymaps.remove(0);
        let saved = apply(SOURCE, &original, &edited).expect("applies");
        assert!(!saved.contains("emacs"), "{saved}");
        assert!(!saved.contains("C-h"), "{saved}");
        assert!(saved.contains("CapsLock = \"LCtrl\""), "{saved}");
        assert!(saved.contains("[macro]"), "{saved}");
    }

    #[test]
    fn general_settings_edit_in_place() {
        let original = draft();
        let mut edited = original.clone();
        edited.macro_delay = "4".to_owned();
        edited.ime.enabled = Some(false);
        edited.ime.duration_ms = "1500".to_owned();
        edited.ime.trigger_keys = "C-Space, W-Space".to_owned();
        let saved = apply(SOURCE, &original, &edited).expect("applies");
        assert!(saved.contains("delay_ms = 4  # ペース\n"), "{saved}");
        assert!(saved.contains("enabled = false\n"), "{saved}");
        assert!(saved.contains("duration_ms = 1500"), "{saved}");
        assert!(
            saved.contains("trigger_keys = [\"C-Space\", \"W-Space\"]  # 切替\n"),
            "{saved}"
        );
    }

    #[test]
    fn the_cursor_tint_round_trips_through_the_draft() {
        // The settings window carried neither of these until v0.9, so a config
        // that used them could be opened and saved without the window ever
        // showing them (owner report 2026-08-10). Reading them is what makes
        // them editable; writing them is what makes an edit stick.
        let source =
            "[ime_indicator]\nchange_cursor_color = true  # 色\ncursor_color = \"#0078d4\"\n";
        let original = parse(source).expect("parses");
        assert_eq!(original.ime.change_cursor_color, Some(true));
        assert_eq!(original.ime.cursor_color, "#0078d4");

        let mut edited = original.clone();
        edited.ime.cursor_color = "#ff8800".to_owned();
        let saved = apply(source, &original, &edited).expect("applies");
        assert!(saved.contains("cursor_color = \"#ff8800\""), "{saved}");
        // The comment on the neighbouring key is untouched, as everywhere else.
        assert!(
            saved.contains("change_cursor_color = true  # 色"),
            "{saved}"
        );
    }

    #[test]
    fn the_cursor_tint_survives_an_edit_that_does_not_touch_it() {
        // What saved v0.8 from losing data: `apply` only writes keys whose
        // draft value changed, so keys the draft did not carry stayed put.
        // Now that the draft does carry them, that has to keep holding.
        let source = "[ime_indicator]\nenabled = true\nchange_cursor_color = true\ncursor_color = \"#0078d4\"\n";
        let original = parse(source).expect("parses");
        let mut edited = original.clone();
        edited.ime.enabled = Some(false);
        let saved = apply(source, &original, &edited).expect("applies");
        assert!(saved.contains("change_cursor_color = true"), "{saved}");
        assert!(saved.contains("cursor_color = \"#0078d4\""), "{saved}");
    }

    #[test]
    fn legacy_delay_spelling_is_preserved_on_save() {
        let source = "macro_delay_ms = 4  # 旧綴り\n";
        let original = parse(source).expect("parses");
        let mut edited = original.clone();
        edited.macro_delay = "6".to_owned();
        let saved = apply(source, &original, &edited).expect("applies");
        assert_eq!(saved, "macro_delay_ms = 6  # 旧綴り\n");
    }

    #[test]
    fn cleared_rules_leave_an_empty_remap_table() {
        let original = draft();
        let mut edited = original.clone();
        edited.keymaps[1].rules.clear();
        let saved = apply(SOURCE, &original, &edited).expect("applies");
        assert!(!saved.contains("CapsLock"), "{saved}");
        assert!(saved.contains("[keymap.remap]"), "{saved}");
    }

    #[test]
    fn application_edit_keeps_per_entry_comments() {
        let original = draft();
        let mut edited = original.clone();
        edited.keymaps[0].application.push("firefox.exe".to_owned());
        let saved = apply(SOURCE, &original, &edited).expect("applies");
        assert!(saved.contains("\"Chrome.exe\",  # ブラウザ"), "{saved}");
        assert!(saved.contains("firefox.exe"), "{saved}");
    }
}
