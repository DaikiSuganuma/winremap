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
        // A one-item list, so the targets are always in the same place
        // whichever form the keymap uses.
        AppFilter::All { .. } => name_list(ui, &[texts.config_apps_all.to_owned()]),
        AppFilter::Names(names) => name_list(ui, names),
    }
    hint(ui, texts.config_apps_case_note);

    if let AppFilter::All { exclude } = &keymap.apps {
        section(ui, texts.config_field_exclude, "exclude");
        note(ui, comments.and_then(|c| c.field("exclude")));
        name_list(ui, exclude);
        hint(ui, texts.config_apps_case_note);
    }

    section(ui, texts.config_rules, "[keymap.remap]");
    rules_ui(ui, keymap, comments);
    macro_note_ui(ui, keymap);

    section(ui, texts.config_notation_title, "");
    notation_help_ui(ui);
}

/// Section titles are bigger than body text and sit under a rule, so a long
/// detail pane reads as parts rather than one wall.
const SECTION_TEXT: f32 = 17.0;
/// The keymap's own name, one step above its sections.
const TITLE_TEXT: f32 = 21.0;

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
    ui.add_space(4.0);
}

/// A small explanatory line under a value, in WinRemap's own words — as
/// opposed to `note`, which shows the user's own comment.
fn hint(ui: &mut egui::Ui, text: &str) {
    ui.add_space(2.0);
    ui.indent("hint", |ui| {
        ui.label(egui::RichText::new(text).weak().small());
    });
}

/// One exe name per line: a comma-joined run of eight is unreadable, which is
/// exactly what a global keymap's exclude list looks like.
fn name_list(ui: &mut egui::Ui, names: &[String]) {
    if names.is_empty() {
        ui.indent("empty-names", |ui| {
            ui.label(egui::RichText::new(i18n::t().config_none).weak());
        });
        return;
    }
    ui.indent("names", |ui| {
        for name in names {
            ui.label(egui::RichText::new(format!("• {name}")).monospace());
        }
    });
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
    egui::Grid::new("rules")
        .striped(true)
        .num_columns(3)
        .min_col_width(120.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(texts.config_rule_input).strong());
            ui.label(egui::RichText::new(texts.config_rule_output).strong());
            ui.label(egui::RichText::new(texts.config_rule_comment).strong());
            ui.end_row();
            for (input, output) in &rules {
                ui.label(egui::RichText::new(input).monospace());
                ui.label(egui::RichText::new(output).monospace());
                let comment = comments.and_then(|c| c.rule(input)).unwrap_or_default();
                ui.label(egui::RichText::new(comment).weak());
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
    ui.add_space(6.0);
    ui.label(egui::RichText::new(i18n::macro_note(delay)).weak());
}

/// What `C-` and friends mean. Always open: the rules right above it are
/// unreadable without it for anyone who did not write them.
fn notation_help_ui(ui: &mut egui::Ui) {
    let texts = i18n::t();
    egui::Grid::new("notation")
        .num_columns(2)
        .min_col_width(60.0)
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
    ui.add_space(4.0);
    ui.label(texts.config_notation_sequence);
    ui.label(texts.config_notation_macro);
    ui.add_space(6.0);
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

    section(ui, texts.config_macro_section, "");
    field(
        ui,
        texts.config_macro_delay,
        "macro_delay_ms",
        &table.macro_delay_ms.to_string(),
    );
    note(ui, comments.general("macro_delay_ms"));
    hint(ui, texts.config_macro_delay_hint);

    section(ui, texts.config_ime_indicator, "[ime_indicator]");
    ime_ui(ui, &table.ime_indicator, comments);
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

    egui::Grid::new("ime-settings")
        .striped(true)
        .num_columns(4)
        .min_col_width(110.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(texts.config_column_item).strong());
            ui.label(egui::RichText::new(texts.config_column_key).strong());
            ui.label(egui::RichText::new(texts.config_column_value).strong());
            ui.label(egui::RichText::new(texts.config_rule_comment).strong());
            ui.end_row();
            for (label, key, value) in &rows {
                ui.label(*label);
                ui.label(egui::RichText::new(*key).monospace().weak());
                ui.label(egui::RichText::new(value).monospace());
                let comment = comments
                    .general(&format!("ime_indicator.{key}"))
                    .unwrap_or_default();
                ui.label(egui::RichText::new(comment).weak());
                ui.end_row();
            }
        });
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

/// The comment the user wrote on that line, if any.
fn note(ui: &mut egui::Ui, comment: Option<&str>) {
    if let Some(comment) = comment {
        ui.indent("note", |ui| {
            ui.label(egui::RichText::new(format!("# {comment}")).weak());
        });
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
