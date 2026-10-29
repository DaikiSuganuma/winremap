//! The save path for a keymap list whose shape changed — a `[[keymap]]`
//! added, removed or reordered — which means handing `toml_edit` new table
//! positions and carrying the file's header along (ADR 0054).

use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

use super::apply::apply_keymap_fields;
use super::toml::{decor_text, output_value, rule_key};
use super::{ConfigDraft, KeymapDraft};

/// `toml_edit` renders tables sorted by their `position`, so adding,
/// removing or reordering `[[keymap]]` tables means handing out new
/// positions. Everything is renumbered with a stride first so the new
/// arrangement has room between the tables that are not moving.
const RENUMBER_STRIDE: isize = 100;

pub(super) fn rebuild_keymaps(doc: &mut DocumentMut, original: &ConfigDraft, edited: &ConfigDraft) {
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
