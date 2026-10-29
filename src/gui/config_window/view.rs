//! The read-only panes (milestone B1): a keymap's targets and rules, and the
//! general settings, as the live table has them.

use std::collections::{HashMap, HashSet};

use eframe::egui;

use super::super::icons::Icon;
use super::nav::keymap_label;
use super::widgets::{app_table, comment_cell, own_note, section, table};
use crate::i18n;
use crate::theme;
use crate::theme::{CELL_PAD, NOTE_GAP};
use winremap::config::comments::{ConfigComments, KeymapComments};
use winremap::ime_indicator_settings::IndicatorSettings;
use winremap::keymap::{
    AppFilter, KeyCombo, Keymap, Output, RemapTable, combo_notation, key_name, vk_display_name,
};

pub(super) fn keymap_ui(
    ui: &mut egui::Ui,
    table: &RemapTable,
    index: usize,
    keymap: &Keymap,
    comments: Option<&KeymapComments>,
) {
    let texts = i18n::t();
    ui.add_space(8.0);
    // No pane title: the breadcrumb and the tree already name the keymap,
    // and a third repetition said nothing new (owner feedback 2026-07-22).
    // The name row and its same-line comment carry what the file says.
    if !keymap.name.is_empty() {
        field(ui, texts.config_field_name, "name", &keymap.name);
        note(ui, comments.and_then(|c| c.field("name")));
    }

    section(ui, Icon::Apps, texts.config_field_apps, "application");
    note(ui, comments.and_then(|c| c.field("application")));
    match &keymap.apps {
        // One row, so the targets are always in the same place whichever
        // form the keymap uses.
        // The file may well have a comment on the `"*"` line; ours is only
        // the fallback for a config that says nothing.
        AppFilter::All { .. } => {
            app_table(ui, "apps", &[texts.config_apps_all.to_owned()], &|_| {
                comments
                    .and_then(|c| c.app("*"))
                    .or(Some(texts.config_apps_all_note))
            })
        }
        AppFilter::Names(names) => app_table(ui, "apps", names, &|name| {
            comments.and_then(|c| c.app(name))
        }),
    }

    if let AppFilter::All { exclude } = &keymap.apps {
        section(ui, Icon::Exclude, texts.config_field_exclude, "exclude");
        note(ui, comments.and_then(|c| c.field("exclude")));
        app_table(ui, "excludes", exclude, &|name| {
            comments.and_then(|c| c.exclude(name))
        });
    }

    section(ui, Icon::Rules, texts.config_rules, "[keymap.remap]");
    rules_ui(ui, table, index, keymap, comments);
    macro_note_ui(ui, keymap);
}

/// Every rule of one keymap as (input, output) display strings, sorted.
///
/// The maps iterate in arbitrary order, so the sort is what keeps the table
/// from reshuffling between frames. Sharing this with the duplicate scan is
/// what makes the two agree on what "the same input" means.
fn rule_rows(keymap: &Keymap) -> Vec<(String, String)> {
    // The keyboard's own spelling, so this table, the comments keyed off it,
    // and the file all say `C-;` for the same rule (ADR 0063).
    let layout = crate::layout::current();
    let notation = |combo: &KeyCombo| combo_notation(combo, &layout);
    let key = |vk: u16| key_name(vk, &layout).unwrap_or_else(|| vk_display_name(vk));
    let mut rules: Vec<(String, String)> = Vec::new();
    for (input, output) in &keymap.exact {
        rules.push((notation(input), render_output(output)));
    }
    for (input_vk, output_vk) in &keymap.bare {
        rules.push((key(*input_vk), key(*output_vk)));
    }
    for (first, seconds) in &keymap.seqs {
        for (second, output) in seconds {
            rules.push((
                format!("{} {}", notation(first), notation(second)),
                render_output(output),
            ));
        }
    }
    rules.sort();
    rules
}

/// Which *other* keymaps bind each input, keyed by the input as displayed.
///
/// A key bound in two keymaps is the thing that is impossible to see in a
/// long config file and surprising at the keyboard: only one of them can win
/// (ADR 0004), and which one is not obvious from reading either in isolation.
fn shared_inputs(table: &RemapTable, index: usize) -> HashMap<String, Vec<String>> {
    let mut owners: HashMap<String, Vec<String>> = HashMap::new();
    let Some(mine) = table.keymaps.get(index) else {
        return owners;
    };
    // Only the inputs this keymap displays can fill a cell, so the scan drops
    // everything else instead of carrying the whole config around.
    let mine: HashSet<String> = rule_rows(mine)
        .into_iter()
        .map(|(input, _)| input)
        .collect();
    for (other, keymap) in table.keymaps.iter().enumerate() {
        if other == index {
            continue;
        }
        for (input, _) in rule_rows(keymap) {
            if !mine.contains(&input) {
                continue;
            }
            owners.entry(input).or_default().push(keymap_label(keymap));
        }
    }
    owners
}

fn rules_ui(
    ui: &mut egui::Ui,
    table_data: &RemapTable,
    index: usize,
    keymap: &Keymap,
    comments: Option<&KeymapComments>,
) {
    let texts = i18n::t();
    let rules = rule_rows(keymap);
    if rules.is_empty() {
        ui.label(egui::RichText::new(texts.config_no_rules).weak());
        return;
    }

    // The column only appears when it has something to say. An always-empty
    // column would cost width on every keymap to serve the rare one.
    let shared = shared_inputs(table_data, index);
    let mut columns = vec![texts.config_rule_input, texts.config_rule_output];
    if !shared.is_empty() {
        columns.push(texts.config_rule_shared);
    }
    columns.push(texts.config_rule_comment);

    table(ui, "rules", &columns, 120.0, |ui| {
        for (input, output) in &rules {
            ui.label(egui::RichText::new(input).monospace());
            ui.label(egui::RichText::new(output).monospace());
            if !shared.is_empty() {
                let owners = shared.get(input);
                ui.label(owners.map(|names| names.join(", ")).unwrap_or_default());
            }
            let comment = comments.and_then(|c| c.rule(input)).unwrap_or_default();
            comment_cell(ui, comment);
            ui.end_row();
        }
    });
    if !shared.is_empty() {
        ui.add_space(NOTE_GAP);
        own_note(ui, texts.config_rule_shared_note);
    }
}

/// Explains the arrows, and where the pacing between them comes from — only
/// for keymaps that actually have a macro, so it does not become noise.
fn macro_note_ui(ui: &mut egui::Ui, keymap: &Keymap) {
    let has_macro = keymap
        .exact
        .values()
        .chain(keymap.seqs.values().flat_map(|seconds| seconds.values()))
        .any(|output| matches!(output, Output::Seq(_)));
    if !has_macro {
        return;
    }
    let delay = crate::hook::REMAP_TABLE
        .load()
        .as_ref()
        .map_or(0, |table| table.macro_delay_ms);
    ui.add_space(NOTE_GAP);
    own_note(ui, &i18n::macro_note(delay));
}

/// A macro is a sequence, so it reads as one: arrows say "then", where the
/// commas the file uses only said "and".
fn render_output(output: &Output) -> String {
    match output {
        Output::Chord(combo) => combo.to_string(),
        Output::Seq(combos) => combos
            .iter()
            .map(|combo| combo.to_string())
            .collect::<Vec<_>>()
            .join(" → "),
    }
}

pub(super) fn general_ui(ui: &mut egui::Ui, table: &RemapTable, comments: &ConfigComments) {
    let texts = i18n::t();
    ui.add_space(8.0);
    // No pane title — same reasoning as the keymap pane: the breadcrumb
    // already says where you are.
    section(ui, Icon::Macro, texts.config_macro_section, "[macro]");
    // The v0.1 spelling still works, so show whichever key the file uses
    // (ADR 0039) - otherwise the comment column would come up empty.
    let (delay_key, delay_comment) = match comments.general("macro.delay_ms") {
        Some(comment) => ("delay_ms", Some(comment)),
        None => match comments.general("macro_delay_ms") {
            Some(comment) => ("macro_delay_ms", Some(comment)),
            None => ("delay_ms", None),
        },
    };
    let mut rows: Vec<(&str, &str, String, Option<&str>)> = vec![(
        texts.config_macro_delay,
        delay_key,
        table.macro_delay_ms.to_string(),
        delay_comment,
    )];
    // Recording is opt-in, so the three keys only earn rows once they exist
    // (ADR 0043); listing "(not configured)" three times would be noise for
    // everyone who does not use the feature.
    if let Some(keys) = table.macro_record.as_ref() {
        rows.push((
            texts.config_macro_record_start,
            "record_start",
            keys.start.to_string(),
            comments.general("macro.record_start"),
        ));
        rows.push((
            texts.config_macro_record_stop,
            "record_stop",
            keys.stop.to_string(),
            comments.general("macro.record_stop"),
        ));
        rows.push((
            texts.config_macro_record_play,
            "record_play",
            keys.play.to_string(),
            comments.general("macro.record_play"),
        ));
    }
    settings_table(ui, "macro-settings", &rows);

    if table.macro_record.is_some() {
        recorded_macro_ui(ui);
    }

    section(ui, Icon::Ime, texts.config_ime_indicator, "[ime_indicator]");
    ime_ui(ui, &table.ime_indicator, comments);
}

/// The macro held in memory right now, rendered the way a config macro is so
/// the two read alike (owner decision 2026-07-21).
///
/// Deliberately outside the settings table: every other row states what the
/// file says, and this one states what memory holds. The note spells that
/// out, because a reader who takes it for a config value would go looking
/// for it in the file and never find it.
pub(super) fn recorded_macro_ui(ui: &mut egui::Ui) {
    let texts = i18n::t();
    let commands = crate::macro_record::recorded();
    let rendered = match commands {
        Some(ref commands) if !commands.is_empty() => commands
            .iter()
            .map(|combo| combo.to_string())
            .collect::<Vec<_>>()
            .join(" → "),
        _ => texts.config_none.to_owned(),
    };
    ui.add_space(NOTE_GAP);
    // Given its own filled box rather than another table row (owner decision
    // 2026-07-21). It is the one value in this window that changes while you
    // watch it, and the surrounding tables all report the file — so looking
    // different is the point, not decoration.
    egui::Frame::new()
        .fill(theme::highlight_fill(ui.visuals()))
        .stroke(theme::highlight_stroke(ui.visuals()))
        .corner_radius(theme::HIGHLIGHT_ROUNDING)
        .inner_margin(egui::Margin::same(theme::HIGHLIGHT_PAD))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(egui::RichText::new(texts.config_macro_recorded).strong());
            ui.add_space(f32::from(CELL_PAD));
            ui.add(
                egui::Label::new(
                    egui::RichText::new(rendered)
                        .monospace()
                        .size(theme::HIGHLIGHT_TEXT),
                )
                .wrap(),
            );
        });
    own_note(ui, texts.config_macro_recorded_note);
}

/// The shared four-column shape for a settings section: what it is, the key
/// in the file, the value in effect, and the user's own note.
fn settings_table(ui: &mut egui::Ui, id: &str, rows: &[(&str, &str, String, Option<&str>)]) {
    let texts = i18n::t();
    let columns = [
        texts.config_column_item,
        texts.config_column_key,
        texts.config_column_value,
        texts.config_rule_comment,
    ];
    table(ui, id, &columns, 110.0, |ui| {
        for (label, key, value, comment) in rows {
            ui.label(*label);
            ui.label(egui::RichText::new(*key).monospace().weak());
            ui.label(egui::RichText::new(value).monospace());
            comment_cell(ui, comment.unwrap_or_default());
            ui.end_row();
        }
    });
}

fn ime_ui(ui: &mut egui::Ui, settings: &IndicatorSettings, comments: &ConfigComments) {
    let texts = i18n::t();
    let triggers = if settings.trigger_keys.is_empty() {
        texts.config_none.to_owned()
    } else {
        settings
            .trigger_keys
            .iter()
            .map(|combo| combo.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mut rows: Vec<(&str, &str, String)> = vec![(
        texts.config_ime_enabled,
        "enabled",
        on_off(settings.enabled),
    )];
    // The rest only describe how the panel looks, which is noise while the
    // feature is off.
    if settings.enabled {
        rows.extend([
            (
                texts.config_ime_duration,
                "duration_ms",
                settings.duration_ms.to_string(),
            ),
            (texts.config_ime_size, "size", settings.size.to_string()),
            (
                texts.config_ime_opacity,
                "opacity",
                settings.opacity.to_string(),
            ),
            (
                texts.config_ime_show_app_name,
                "show_app_name",
                on_off(settings.show_app_name),
            ),
            (texts.config_ime_triggers, "trigger_keys", triggers),
        ]);
    }

    // Outside the `enabled` block on purpose. The tint is independent of the
    // panel (ADR 0067) — it works with `enabled = false`, and acceptance M-6
    // is the item that checks exactly that. Folding it in with the panel's
    // appearance would tell the reader it needs the panel switched on.
    rows.push((
        texts.config_ime_change_cursor_color,
        "change_cursor_color",
        on_off(settings.change_cursor_color),
    ));
    if settings.change_cursor_color {
        let (r, g, b) = settings.cursor_color;
        rows.push((
            texts.config_ime_cursor_color,
            "cursor_color",
            // The spelling the config accepts, so what is shown can be typed
            // back in (`parse_hex_color` takes `#rrggbb` and nothing else).
            format!("#{r:02x}{g:02x}{b:02x}"),
        ));
    }

    let rows: Vec<(&str, &str, String, Option<&str>)> = rows
        .into_iter()
        .map(|(label, key, value)| {
            let comment = comments.general(&format!("ime_indicator.{key}"));
            (label, key, value, comment)
        })
        .collect();
    settings_table(ui, "ime-settings", &rows);
}

fn on_off(value: bool) -> String {
    let texts = i18n::t();
    if value {
        texts.config_on
    } else {
        texts.config_off
    }
    .to_owned()
}

/// One "label  key = value" row. The TOML key is shown next to the friendly
/// label so the window doubles as a map back to the file.
fn field(ui: &mut egui::Ui, label: &str, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.label(egui::RichText::new(key).monospace().weak());
        ui.label(egui::RichText::new("=").weak());
        ui.label(egui::RichText::new(value).monospace());
    });
}

/// The comment the user wrote on that line, if any, kept clear of the list it
/// introduces so the two do not read as one block.
///
/// Shown without the `#`: it is a note here, not a line of TOML, and the rule
/// table's comment column has never carried the marker either (owner feedback
/// 2026-07-26).
fn note(ui: &mut egui::Ui, comment: Option<&str>) {
    if let Some(comment) = comment {
        ui.indent("note", |ui| {
            ui.label(egui::RichText::new(comment).weak());
        });
        ui.add_space(NOTE_GAP);
    }
}
