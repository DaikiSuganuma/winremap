//! The settings window: what WinRemap is currently doing, per keymap.
//!
//! Milestone B1 is read-only (docs/v0.2/04_config-gui-design.md §7). It shows
//! the *live* table — the one the hook resolves against — rather than
//! re-reading the file, so what is on screen is always what is in effect, and
//! a tray reload is reflected without any refresh of its own. B2 introduces
//! the file-backed draft that editing needs.
//!
//! The one thing the live table cannot supply is the comments the user wrote
//! next to their rules, and a long rule list is unreadable without them; those
//! come from a second, formatting-preserving read of the file
//! (`config::comments`), refreshed whenever the table is swapped.

use std::path::Path;
use std::sync::Arc;

use eframe::egui;

use crate::i18n;
use winremap::config::comments::{ConfigComments, KeymapComments};
use winremap::ime_indicator_settings::IndicatorSettings;
use winremap::keymap::{AppFilter, Keymap, Output, RemapTable, vk_display_name};

/// Which entry the left list has selected.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Selection {
    #[default]
    General,
    Keymap(usize),
}

#[derive(Default)]
pub struct ConfigWindow {
    selection: Selection,
    comments: ConfigComments,
    /// Identifies the table the comments were read for, so the file is only
    /// re-read when a reload swaps in a new one (ADR 0003) rather than every
    /// frame. Compared, never dereferenced.
    comments_for: Option<usize>,
}

impl ConfigWindow {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let texts = i18n::t();
        let path = super::config_path()
            .lock()
            .map(|path| path.clone())
            .unwrap_or_default();
        // A snapshot for the whole frame: the hook may swap the table at any
        // moment (ADR 0003), and the list and the detail pane have to agree.
        let table = crate::hook::REMAP_TABLE.load_full();
        self.sync_comments(table.as_ref(), &path);

        egui::Panel::top("config-header").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(texts.config_window_file);
                ui.label(egui::RichText::new(path.display().to_string()).monospace());
                if ui.link(texts.config_window_open_in_editor).clicked() {
                    open_in_default_editor(&path);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} v{}",
                            texts.app_name,
                            env!("CARGO_PKG_VERSION")
                        ))
                        .weak(),
                    );
                });
            });
            ui.add_space(2.0);
            ui.label(egui::RichText::new(texts.config_window_readonly).weak());
            ui.add_space(6.0);
        });

        let Some(table) = table else {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label(texts.config_window_no_config);
            });
            return;
        };

        egui::Panel::left("config-list")
            .default_size(220.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.list_ui(ui, &table));
            });

        // Only alongside a rule list, the one thing it explains — on the
        // general page it would be a legend for nothing. Explorer's details
        // pane behaves the same way.
        if self.shows_rules(&table) {
            egui::Panel::right("config-notation")
                .default_size(240.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, notation_help_ui);
                });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.selection {
                    Selection::General => general_ui(ui, &table, &self.comments),
                    // The table can shrink under us on a reload; fall back to
                    // the general page rather than panicking on the index.
                    Selection::Keymap(index) => match table.keymaps.get(index) {
                        Some(keymap) => keymap_ui(ui, keymap, self.comments.keymap(index)),
                        None => general_ui(ui, &table, &self.comments),
                    },
                });
        });
    }

    /// Whether the detail pane is currently showing a keymap. Mirrors the
    /// fallback in `ui`: an index the reload left dangling shows General.
    fn shows_rules(&self, table: &RemapTable) -> bool {
        matches!(self.selection, Selection::Keymap(index) if table.keymaps.get(index).is_some())
    }

    fn sync_comments(&mut self, table: Option<&Arc<RemapTable>>, path: &Path) {
        let current = table.map(|table| Arc::as_ptr(table) as usize);
        if current == self.comments_for {
            return;
        }
        self.comments_for = current;
        self.comments = winremap::config::comments::read(path);
    }

    fn list_ui(&mut self, ui: &mut egui::Ui, table: &RemapTable) {
        let texts = i18n::t();
        ui.add_space(4.0);
        ui.selectable_value(
            &mut self.selection,
            Selection::General,
            texts.config_general,
        );
        ui.add_space(8.0);
        ui.label(egui::RichText::new(texts.config_keymaps).strong());
        if table.keymaps.is_empty() {
            ui.label(egui::RichText::new(texts.config_no_keymaps).weak());
        }
        for (index, keymap) in table.keymaps.iter().enumerate() {
            ui.selectable_value(
                &mut self.selection,
                Selection::Keymap(index),
                keymap_label(keymap),
            );
        }
    }
}

/// List entry text: the section's `name`, or its target when it has none.
fn keymap_label(keymap: &Keymap) -> String {
    if !keymap.name.is_empty() {
        return keymap.name.clone();
    }
    match &keymap.apps {
        AppFilter::All { .. } => i18n::t().config_apps_all.to_owned(),
        AppFilter::Names(names) => names.join(", "),
    }
}

fn keymap_ui(ui: &mut egui::Ui, keymap: &Keymap, comments: Option<&KeymapComments>) {
    let texts = i18n::t();
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(keymap_label(keymap))
            .size(TITLE_TEXT)
            .strong(),
    );
    if !keymap.name.is_empty() {
        field(ui, texts.config_field_name, "name", &keymap.name);
        note(ui, comments.and_then(|c| c.field("name")));
    }

    section(ui, texts.config_field_apps, "application");
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
        section(ui, texts.config_field_exclude, "exclude");
        note(ui, comments.and_then(|c| c.field("exclude")));
        app_table(ui, "excludes", exclude, &|name| {
            comments.and_then(|c| c.exclude(name))
        });
    }

    section(ui, texts.config_rules, "[keymap.remap]");
    rules_ui(ui, keymap, comments);
    macro_note_ui(ui, keymap);
}

/// Section titles are bigger than body text and sit under a rule, so a long
/// detail pane reads as parts rather than one wall.
const SECTION_TEXT: f32 = 17.0;
/// The keymap's own name, one step above its sections.
const TITLE_TEXT: f32 = 21.0;
/// Room around cell text, and the gap that keeps a note off the table it
/// belongs to. Applied as grid spacing, so half of it lands on each side of
/// the gap between two cells.
const CELL_PAD: f32 = 4.0;
/// A note reads as belonging to the table it sits under only if there is a
/// clear break between them.
const NOTE_GAP: f32 = 8.0;

/// The shared look for every table in this window: a hairline border, a
/// reverse-coloured header row, and room around the text.
///
/// egui's `Grid` has no notion of a header, so row 0 is coloured through
/// `with_row_color` — the same hook the zebra striping uses, which is why the
/// stripes are spelled out here rather than left to `striped`. Colouring that
/// way rather than per cell is what makes the header a full-width band.
fn table(
    ui: &mut egui::Ui,
    id: &str,
    columns: &[&str],
    min_col_width: f32,
    rows: impl FnOnce(&mut egui::Ui),
) {
    let border = ui.visuals().widgets.noninteractive.bg_stroke;
    // The header's text takes the window's background colour, which is what
    // "reversed" means here — and it follows the light/dark theme for free.
    let header_text = ui.visuals().extreme_bg_color;
    egui::Frame::new()
        .stroke(border)
        .inner_margin(CELL_PAD)
        .show(ui, |ui| {
            egui::Grid::new(id)
                .num_columns(columns.len())
                .min_col_width(min_col_width)
                .spacing([CELL_PAD * 4.0, CELL_PAD * 2.0])
                .with_row_color(|row, style| match row {
                    0 => Some(style.visuals.text_color()),
                    row if row % 2 == 1 => Some(style.visuals.faint_bg_color),
                    _ => None,
                })
                .show(ui, |ui| {
                    for column in columns {
                        ui.label(egui::RichText::new(*column).strong().color(header_text));
                    }
                    ui.end_row();
                    rows(ui);
                });
        });
}

fn section(ui: &mut egui::Ui, title: &str, key: &str) {
    ui.add_space(14.0);
    ui.separator();
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).size(SECTION_TEXT).strong());
        if !key.is_empty() {
            ui.label(egui::RichText::new(key).monospace().weak());
        }
    });
    ui.add_space(CELL_PAD);
}

/// Exe names one per row, each with whatever the user wrote next to it in
/// the file: a comma-joined run of eight is unreadable, which is exactly what
/// a global keymap's exclude list looks like.
fn app_table<'a>(
    ui: &mut egui::Ui,
    id: &str,
    names: &[String],
    // The lifetime says the comment borrows from the comment set, not from
    // the name it was looked up by.
    comment_of: &dyn Fn(&str) -> Option<&'a str>,
) {
    let texts = i18n::t();
    if names.is_empty() {
        ui.label(egui::RichText::new(texts.config_none).weak());
    } else {
        let columns = [texts.config_column_app, texts.config_rule_comment];
        table(ui, id, &columns, 180.0, |ui| {
            for name in names {
                ui.label(egui::RichText::new(name).monospace());
                ui.label(comment_of(name).unwrap_or_default());
                ui.end_row();
            }
        });
    }
    ui.add_space(NOTE_GAP);
    own_note(ui, texts.config_apps_case_note);
}

/// A line in WinRemap's own words, marked so it is never mistaken for the
/// user's comment. `note` shows the latter.
fn own_note(ui: &mut egui::Ui, text: &str) {
    ui.label(format!("{} {text}", i18n::t().note_marker));
}

fn rules_ui(ui: &mut egui::Ui, keymap: &Keymap, comments: Option<&KeymapComments>) {
    let texts = i18n::t();
    // HashMap iteration order is arbitrary, so sort — a list that reshuffles
    // between frames would be unreadable.
    let mut rules: Vec<(String, String)> = Vec::new();
    for (input, output) in &keymap.exact {
        rules.push((input.to_string(), render_output(output)));
    }
    for (input_vk, output_vk) in &keymap.bare {
        rules.push((vk_display_name(*input_vk), vk_display_name(*output_vk)));
    }
    for (first, seconds) in &keymap.seqs {
        for (second, output) in seconds {
            rules.push((format!("{first} {second}"), render_output(output)));
        }
    }
    rules.sort();

    if rules.is_empty() {
        ui.label(egui::RichText::new(texts.config_no_rules).weak());
        return;
    }
    let columns = [
        texts.config_rule_input,
        texts.config_rule_output,
        texts.config_rule_comment,
    ];
    table(ui, "rules", &columns, 120.0, |ui| {
        for (input, output) in &rules {
            ui.label(egui::RichText::new(input).monospace());
            ui.label(egui::RichText::new(output).monospace());
            let comment = comments.and_then(|c| c.rule(input)).unwrap_or_default();
            ui.label(comment);
            ui.end_row();
        }
    });
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

/// What `C-` and friends mean. Lives in its own pane beside the rules, which
/// are unreadable without it for anyone who did not write them, and always
/// open — a legend behind a disclosure triangle helps nobody.
///
/// No border or header here: it is a legend, not data, and giving it the same
/// weight as the rule table would make the pane compete with it.
fn notation_help_ui(ui: &mut egui::Ui) {
    let texts = i18n::t();
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(texts.config_notation_title)
            .size(SECTION_TEXT)
            .strong(),
    );
    ui.add_space(NOTE_GAP);
    egui::Grid::new("notation")
        .num_columns(2)
        .min_col_width(60.0)
        .spacing([CELL_PAD * 4.0, CELL_PAD * 2.0])
        .show(ui, |ui| {
            for (prefix, meaning) in [
                ("C-", texts.config_notation_ctrl),
                ("A-", texts.config_notation_alt),
                ("S-", texts.config_notation_shift),
                ("W-", texts.config_notation_win),
            ] {
                ui.label(egui::RichText::new(prefix).monospace());
                ui.label(meaning);
                ui.end_row();
            }
        });
    ui.add_space(NOTE_GAP);
    ui.label(texts.config_notation_sequence);
    ui.add_space(CELL_PAD);
    ui.label(texts.config_notation_macro);
    ui.add_space(NOTE_GAP);
    if ui.link(texts.config_help_link).clicked() {
        super::log::action(texts.config_help_link);
        super::win32::open_url(i18n::help_url());
    }
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

fn general_ui(ui: &mut egui::Ui, table: &RemapTable, comments: &ConfigComments) {
    let texts = i18n::t();
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(texts.config_general)
            .size(TITLE_TEXT)
            .strong(),
    );

    section(ui, texts.config_macro_section, "[macro]");
    // The v0.1 spelling still works, so show whichever key the file uses
    // (ADR 0039) - otherwise the comment column would come up empty.
    let (delay_key, delay_comment) = match comments.general("macro.delay_ms") {
        Some(comment) => ("delay_ms", Some(comment)),
        None => match comments.general("macro_delay_ms") {
            Some(comment) => ("macro_delay_ms", Some(comment)),
            None => ("delay_ms", None),
        },
    };
    settings_table(
        ui,
        "macro-settings",
        &[(
            texts.config_macro_delay,
            delay_key,
            table.macro_delay_ms.to_string(),
            delay_comment,
        )],
    );

    section(ui, texts.config_ime_indicator, "[ime_indicator]");
    ime_ui(ui, &table.ime_indicator, comments);
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
            ui.label(comment.unwrap_or_default());
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
fn note(ui: &mut egui::Ui, comment: Option<&str>) {
    if let Some(comment) = comment {
        ui.indent("note", |ui| {
            ui.label(egui::RichText::new(format!("# {comment}")).weak());
        });
        ui.add_space(NOTE_GAP);
    }
}

/// Hands the file to whatever the user associated with `.toml`. Moved here
/// from the tray when the menu item became "Settings" (owner decision
/// 2026-07-21).
///
/// A failure is reported rather than swallowed: the button doing nothing at
/// all is exactly the bug ADR 0038 came from.
fn open_in_default_editor(path: &Path) {
    super::log::action(&i18n::action_open_editor(&path.display().to_string()));
    if !super::win32::open_in_default_editor(path) {
        crate::notify::error(&i18n::open_editor_failed(&path.display().to_string()));
    }
}
