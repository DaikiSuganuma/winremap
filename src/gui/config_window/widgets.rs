//! The primitives every pane is built from: the bordered table with its
//! reverse-coloured header, section headings, and the two kinds of note.

use eframe::egui;

use super::super::icons::{self, Icon};
use crate::i18n;
use crate::theme;
use crate::theme::{CELL_PAD, EDGE_PAD, NOTE_GAP, SECTION_GAP, SECTION_TEXT};

/// Grid spacing is the gap *between* cells, so it is twice the padding each
/// cell gets. Columns get the wider gap: adjacent values need more to read
/// apart than stacked rows do.
pub(super) fn cell_spacing() -> egui::Vec2 {
    let pad = f32::from(CELL_PAD);
    egui::vec2(pad * 4.0, pad * 2.0)
}

/// The shared look for every table in this window: a thin border, a
/// reverse-coloured header row, and room around the text.
///
/// egui's `Grid` has no notion of a header, so row 0 is coloured through
/// `with_row_color` — the same hook the zebra striping uses, which is why the
/// stripes are spelled out here rather than left to `striped`. Colouring that
/// way rather than per cell is what makes the header a full-width band.
pub(super) fn table(
    ui: &mut egui::Ui,
    id: &str,
    columns: &[&str],
    min_col_width: f32,
    rows: impl FnOnce(&mut egui::Ui),
) {
    let border = theme::table_border(ui.visuals());
    // The header's text takes the window's background colour, which is what
    // "reversed" means here — and it follows the light/dark theme for free.
    let header_text = theme::table_header_text(ui.visuals());
    egui::Frame::new()
        .stroke(border)
        .inner_margin(egui::Margin::symmetric(EDGE_PAD, CELL_PAD))
        .show(ui, |ui| {
            // Without this the frame shrinks to its widest row, and a table of
            // short values sits in the corner of the pane instead of filling
            // it.
            ui.set_min_width(ui.available_width());
            egui::Grid::new(id)
                .num_columns(columns.len())
                .min_col_width(min_col_width)
                .spacing(cell_spacing())
                .with_row_color(|row, style| match row {
                    0 => Some(theme::table_header_bg(&style.visuals)),
                    row if row % 2 == 1 => Some(theme::table_stripe(&style.visuals)),
                    _ => None,
                })
                .show(ui, |ui| {
                    let last = columns.len().saturating_sub(1);
                    for (index, column) in columns.iter().enumerate() {
                        let text = egui::RichText::new(*column).strong().color(header_text);
                        if index == last {
                            // The last column claims the rest of the row, which
                            // is what makes the header a band across the whole
                            // table and gives `comment_cell` room to fold into.
                            ui.horizontal(|ui| {
                                ui.label(text);
                                ui.add_space(ui.available_width());
                            });
                        } else {
                            ui.label(text);
                        }
                    }
                    ui.end_row();
                    rows(ui);
                });
        });
}

/// The user's own comment, always the last column. Wrapping has to be asked
/// for: a `Grid` gives its cells unbounded width, so a long comment would
/// otherwise stretch the table past the edge of the window rather than fold.
pub(super) fn comment_cell(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(text).wrap());
}

/// Sections are separated by whitespace alone: the pane had collected enough
/// horizontal rules — table borders, dividers — that they stopped reading as
/// structure and started reading as clutter (owner decision 2026-07-21). The
/// gap is doubled to carry the break the line used to.
pub(super) fn section(ui: &mut egui::Ui, icon: Icon, title: &str, key: &str) {
    ui.add_space(SECTION_GAP * 2.0);
    ui.horizontal(|ui| {
        icons::show(ui, icon, SECTION_TEXT);
        ui.label(egui::RichText::new(title).size(SECTION_TEXT).strong());
        if !key.is_empty() {
            ui.label(egui::RichText::new(key).monospace().weak());
        }
    });
    ui.add_space(f32::from(CELL_PAD));
}

/// Exe names one per row, each with whatever the user wrote next to it in
/// the file: a comma-joined run of eight is unreadable, which is exactly what
/// a global keymap's exclude list looks like.
pub(super) fn app_table<'a>(
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
                comment_cell(ui, comment_of(name).unwrap_or_default());
                ui.end_row();
            }
        });
    }
    ui.add_space(NOTE_GAP);
    own_note(ui, texts.config_apps_case_note);
}

/// A line in WinRemap's own words, marked so it is never mistaken for the
/// user's comment. `note` shows the latter.
pub(super) fn own_note(ui: &mut egui::Ui, text: &str) {
    ui.label(format!("{} {text}", i18n::t().note_marker));
}
