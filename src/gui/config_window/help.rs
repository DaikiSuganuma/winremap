//! The two explanations the window carries: the key-name reference behind a
//! `?` button (screen design §6.5) and the always-open notation legend
//! beside the rules.

use eframe::egui;

use super::super::icons::{self, Icon};
use super::widgets::cell_spacing;
use crate::i18n;
use crate::theme::{CELL_PAD, NOTE_GAP, SECTION_TEXT};
use winremap::keymap::SPECIAL_KEY_NAMES;

/// The key-name reference behind a `?` button (screen design §6.5),
/// generated from the parser's own list so the two cannot drift.
pub(super) fn key_names_help(ui: &mut egui::Ui, salt: &str) {
    let texts = i18n::t();
    let response = icons::icon_button(ui, Icon::Notation).on_hover_text(texts.config_keys_title);
    egui::Popup::from_toggle_button_response(&response)
        .id(egui::Id::new(salt))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(280.0);
            ui.label(egui::RichText::new(texts.config_keys_title).strong());
            ui.add_space(f32::from(CELL_PAD));
            egui::Grid::new("key-names")
                .num_columns(2)
                .spacing(cell_spacing())
                .show(ui, |ui| {
                    ui.label(texts.config_keys_mods);
                    ui.label(egui::RichText::new("C-  A-  S-  W-").monospace());
                    ui.end_row();
                    ui.label(texts.config_keys_chars);
                    ui.label(egui::RichText::new(texts.config_keys_chars_list).monospace());
                    ui.end_row();
                    ui.label(texts.config_keys_function);
                    ui.label(egui::RichText::new(texts.config_keys_function_list).monospace());
                    ui.end_row();
                    ui.label(texts.config_keys_special);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(SPECIAL_KEY_NAMES.join(" ")).monospace(),
                        )
                        .wrap(),
                    );
                    ui.end_row();
                    // Read off the keyboard rather than listed: `;` and `:`
                    // are on different keys depending on the layout, so a
                    // fixed list would be wrong for half the readers
                    // (ADR 0063).
                    ui.label(texts.config_keys_symbols);
                    let symbols = crate::layout::current().symbol_keys();
                    let listed = if symbols.is_empty() {
                        texts.config_keys_symbols_unknown.to_owned()
                    } else {
                        symbols
                            .iter()
                            .map(|(face, _)| face.to_string())
                            .collect::<Vec<_>>()
                            .join(" ")
                    };
                    ui.add(egui::Label::new(egui::RichText::new(listed).monospace()).wrap());
                    ui.end_row();
                });
            ui.add_space(f32::from(CELL_PAD));
            ui.add(
                egui::Label::new(egui::RichText::new(texts.config_keys_symbols_note).small())
                    .wrap(),
            );
        });
}

/// What `C-` and friends mean. Lives in its own pane beside the rules, which
/// are unreadable without it for anyone who did not write them, and always
/// open — a legend behind a disclosure triangle helps nobody.
///
/// No border or header here: it is a legend, not data, and giving it the same
/// weight as the rule table would make the pane compete with it.
pub(super) fn notation_help_ui(ui: &mut egui::Ui) {
    let texts = i18n::t();
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        icons::show(ui, Icon::Notation, SECTION_TEXT);
        ui.label(
            egui::RichText::new(texts.config_notation_title)
                .size(SECTION_TEXT)
                .strong(),
        );
    });
    ui.add_space(NOTE_GAP);
    egui::Grid::new("notation")
        .num_columns(2)
        .min_col_width(60.0)
        .spacing(cell_spacing())
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
    ui.add_space(f32::from(CELL_PAD));
    ui.label(texts.config_notation_macro);
    ui.add_space(NOTE_GAP);
    if icons::link(ui, Icon::Link, texts.config_help_link) {
        crate::gui::log::action(texts.config_help_link);
        crate::gui::win32::open_url(i18n::help_url());
    }
}
