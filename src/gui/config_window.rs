//! The settings window: what WinRemap is currently doing, per keymap.
//!
//! Milestone B1 is read-only (docs/v0.2/04_config-gui-design.md §7). It shows
//! the *live* table — the one the hook resolves against — rather than
//! re-reading the file, so what is on screen is always what is in effect, and
//! a tray reload is reflected without any refresh of its own. B2 introduces
//! the file-backed draft that editing needs.

use std::path::Path;

use eframe::egui;

use crate::i18n;
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

        egui::Panel::top("config-header").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(texts.config_window_file);
                ui.label(egui::RichText::new(path.display().to_string()).monospace());
                if ui.button(texts.config_window_open_in_editor).clicked() {
                    open_in_default_editor(&path);
                }
            });
            ui.add_space(2.0);
            ui.label(egui::RichText::new(texts.config_window_readonly).weak());
            ui.add_space(4.0);
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
                    Selection::General => general_ui(ui, &table),
                    // The table can shrink under us on a reload; fall back to
                    // the general page rather than panicking on the index.
                    Selection::Keymap(index) => match table.keymaps.get(index) {
                        Some(keymap) => keymap_ui(ui, keymap),
                        None => general_ui(ui, &table),
                    },
                });
        });
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

fn keymap_ui(ui: &mut egui::Ui, keymap: &Keymap) {
    let texts = i18n::t();
    ui.add_space(8.0);
    ui.heading(keymap_label(keymap));
    ui.add_space(8.0);

    match &keymap.apps {
        AppFilter::All { exclude } => {
            field(ui, texts.config_field_apps, texts.config_apps_all);
            let excluded = if exclude.is_empty() {
                texts.config_none.to_owned()
            } else {
                exclude.join(", ")
            };
            field(ui, texts.config_field_exclude, &excluded);
        }
        AppFilter::Names(names) => field(ui, texts.config_field_apps, &names.join(", ")),
    }

    ui.add_space(12.0);
    ui.label(egui::RichText::new(texts.config_rules).strong());

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
        .num_columns(2)
        .min_col_width(160.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(texts.config_rule_input).strong());
            ui.label(egui::RichText::new(texts.config_rule_output).strong());
            ui.end_row();
            for (input, output) in &rules {
                ui.label(egui::RichText::new(input).monospace());
                ui.label(egui::RichText::new(output).monospace());
                ui.end_row();
            }
        });
}

/// Macro outputs are a list of chords; a comma keeps them on one line and
/// matches how the design doc has the editor accept them.
fn render_output(output: &Output) -> String {
    match output {
        Output::Chord(combo) => combo.to_string(),
        Output::Seq(combos) => combos
            .iter()
            .map(|combo| combo.to_string())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn general_ui(ui: &mut egui::Ui, table: &RemapTable) {
    let texts = i18n::t();
    ui.add_space(8.0);
    ui.heading(texts.config_general);
    ui.add_space(8.0);
    field(
        ui,
        texts.config_macro_delay,
        &table.macro_delay_ms.to_string(),
    );

    ui.add_space(12.0);
    ui.label(egui::RichText::new(texts.config_ime_indicator).strong());
    ime_ui(ui, &table.ime_indicator);
}

fn ime_ui(ui: &mut egui::Ui, settings: &IndicatorSettings) {
    let texts = i18n::t();
    field(ui, texts.config_ime_enabled, &on_off(settings.enabled));
    if !settings.enabled {
        return;
    }
    field(
        ui,
        texts.config_ime_duration,
        &settings.duration_ms.to_string(),
    );
    field(ui, texts.config_ime_size, &settings.size.to_string());
    field(ui, texts.config_ime_opacity, &settings.opacity.to_string());
    field(
        ui,
        texts.config_ime_show_app_name,
        &on_off(settings.show_app_name),
    );
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
    field(ui, texts.config_ime_triggers, &triggers);
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

/// One "label: value" row, with the value monospaced so config notation and
/// exe names line up.
fn field(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.label(egui::RichText::new(value).monospace());
    });
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
