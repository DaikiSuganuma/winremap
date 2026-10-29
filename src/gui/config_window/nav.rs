//! The navigation tree on the left (v0.4 screen design §3) and the
//! breadcrumb over the detail pane (§4.1), in both view and edit mode.

use eframe::egui;

use super::super::icons::{self, Icon};
use super::{ConfigWindow, Selection};
use crate::i18n;
use crate::theme;
use crate::theme::{CELL_PAD, NOTE_GAP};
use winremap::config::draft::{ConfigDraft, KeymapDraft};
use winremap::keymap::{AppFilter, Keymap, RemapTable};

impl ConfigWindow {
    /// The navigation tree (v0.4 screen design §3): icons, an indent for the
    /// keymaps under their heading, no expand/collapse.
    pub(super) fn list_ui(&mut self, ui: &mut egui::Ui, table: &RemapTable) {
        let texts = i18n::t();
        ui.add_space(4.0);
        if nav_row(
            ui,
            self.selection == Selection::General,
            false,
            Icon::Gear,
            texts.config_general,
        )
        .clicked()
        {
            self.selection = Selection::General;
        }
        ui.add_space(8.0);
        // The group heading draws the tree's hierarchy; it is not a
        // destination itself.
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            icons::show(ui, Icon::Keyboard, theme::body_icon_size(ui));
            ui.label(egui::RichText::new(texts.config_keymaps).strong());
        });
        if table.keymaps.is_empty() {
            ui.label(egui::RichText::new(texts.config_no_keymaps).weak());
        }
        for (index, keymap) in table.keymaps.iter().enumerate() {
            if nav_row(
                ui,
                self.selection == Selection::Keymap(index),
                true,
                Icon::Apps,
                &keymap_label(keymap),
            )
            .clicked()
            {
                self.selection = Selection::Keymap(index);
            }
        }
    }
}

/// One row of the navigation tree: an icon, a label, and — when selected —
/// a full-width grey fill like Explorer's (owner decision 2026-07-22).
/// egui's `selectable_value` highlights only the text, hence hand-drawn.
fn nav_row(
    ui: &mut egui::Ui,
    selected: bool,
    indented: bool,
    icon: Icon,
    label: &str,
) -> egui::Response {
    let size = egui::vec2(ui.available_width(), theme::NAV_ROW_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        if selected {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(2),
                theme::sidebar_selection_fill(ui.visuals()),
            );
        } else if response.hovered() {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(2),
                theme::sidebar_hover_fill(ui.visuals()),
            );
        }
        let inner = rect.shrink2(egui::vec2(6.0, 0.0));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        // The label must not be text-selectable: egui's selectable labels
        // claim the pointer for drag-selection, which swallowed clicks on
        // the text and left only the blank end of the row clickable (owner
        // feedback 2026-07-22). The row is a button; its text is not prose.
        child.style_mut().interaction.selectable_labels = false;
        if indented {
            child.add_space(theme::NAV_INDENT);
        }
        let icon_size = theme::body_icon_size(&child);
        icons::show(&mut child, icon, icon_size);
        child.add_space(4.0);
        child.label(label);
    }
    response
}

/// The breadcrumb over the detail pane: where you are, Explorer-style
/// (v0.4 screen design §4.1). Display only — the tree does the moving.
pub(super) fn breadcrumb_ui(ui: &mut egui::Ui, table: &RemapTable, selection: Selection) {
    let texts = i18n::t();
    ui.add_space(f32::from(CELL_PAD));
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(texts.config_breadcrumb_root).weak());
        ui.label(egui::RichText::new(">").weak());
        match selection {
            Selection::General => {
                ui.label(texts.config_general);
            }
            Selection::Keymap(index) => match table.keymaps.get(index) {
                Some(keymap) => {
                    ui.label(egui::RichText::new(texts.config_keymaps).weak());
                    ui.label(egui::RichText::new(">").weak());
                    ui.label(keymap_label(keymap));
                }
                // Mirrors the detail pane's reload fallback.
                None => {
                    ui.label(texts.config_general);
                }
            },
        }
    });
}

/// The navigation tree in edit mode: the same rows over the draft, plus the
/// keymap add/delete/reorder controls (screen design §3.1).
pub(super) fn edit_list_ui(ui: &mut egui::Ui, selection: &mut Selection, draft: &mut ConfigDraft) {
    let texts = i18n::t();
    ui.add_space(4.0);
    if nav_row(
        ui,
        *selection == Selection::General,
        false,
        Icon::Gear,
        texts.config_general,
    )
    .clicked()
    {
        *selection = Selection::General;
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        icons::show(ui, Icon::Keyboard, theme::body_icon_size(ui));
        ui.label(egui::RichText::new(texts.config_keymaps).strong());
    });
    if draft.keymaps.is_empty() {
        ui.label(egui::RichText::new(texts.config_no_keymaps).weak());
    }
    for (index, keymap) in draft.keymaps.iter().enumerate() {
        if nav_row(
            ui,
            *selection == Selection::Keymap(index),
            true,
            Icon::Apps,
            &keymap_draft_label(keymap),
        )
        .clicked()
        {
            *selection = Selection::Keymap(index);
        }
    }
    ui.add_space(NOTE_GAP);
    let selected = match *selection {
        Selection::Keymap(index) if index < draft.keymaps.len() => Some(index),
        _ => None,
    };
    ui.horizontal(|ui| {
        if icons::icon_button(ui, Icon::Plus)
            .on_hover_text(texts.config_keymap_add)
            .clicked()
        {
            draft.keymaps.push(KeymapDraft::default());
            *selection = Selection::Keymap(draft.keymaps.len() - 1);
        }
        // Only a selected keymap can be deleted or moved; General and the
        // heading are not list entries.
        ui.add_enabled_ui(selected.is_some(), |ui| {
            if icons::icon_button(ui, Icon::Dash)
                .on_hover_text(texts.config_keymap_remove)
                .clicked()
                && let Some(index) = selected
            {
                draft.keymaps.remove(index);
                *selection = if draft.keymaps.is_empty() {
                    Selection::General
                } else {
                    Selection::Keymap(index.min(draft.keymaps.len() - 1))
                };
            }
            if icons::icon_button(ui, Icon::ArrowUp)
                .on_hover_text(texts.config_move_up)
                .clicked()
                && let Some(index) = selected
                && index > 0
                && index < draft.keymaps.len()
            {
                draft.keymaps.swap(index - 1, index);
                *selection = Selection::Keymap(index - 1);
            }
            if icons::icon_button(ui, Icon::ArrowDown)
                .on_hover_text(texts.config_move_down)
                .clicked()
                && let Some(index) = selected
                && index + 1 < draft.keymaps.len()
            {
                draft.keymaps.swap(index, index + 1);
                *selection = Selection::Keymap(index + 1);
            }
        });
    });
}

/// List label for a draft keymap; mirrors `keymap_label` for compiled ones.
fn keymap_draft_label(keymap: &KeymapDraft) -> String {
    let texts = i18n::t();
    if !keymap.name.is_empty() {
        return keymap.name.clone();
    }
    if keymap.application.iter().any(|app| app == "*") {
        return texts.config_apps_all.to_owned();
    }
    if keymap.application.is_empty() {
        return texts.config_none.to_owned();
    }
    keymap.application.join(", ")
}

/// The edit-mode breadcrumb: the same crumbs over the draft, with the
/// editing banner at the right end (screen design §4.1) — the header colour
/// says it, this spells it out.
pub(super) fn breadcrumb_edit_ui(ui: &mut egui::Ui, draft: &ConfigDraft, selection: Selection) {
    let texts = i18n::t();
    ui.add_space(f32::from(CELL_PAD));
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(texts.config_breadcrumb_root).weak());
        ui.label(egui::RichText::new(">").weak());
        match selection {
            Selection::Keymap(index) if index < draft.keymaps.len() => {
                ui.label(egui::RichText::new(texts.config_keymaps).weak());
                ui.label(egui::RichText::new(">").weak());
                ui.label(keymap_draft_label(&draft.keymaps[index]));
            }
            _ => {
                ui.label(texts.config_general);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let warn = ui.visuals().warn_fg_color;
            ui.label(egui::RichText::new(texts.config_edit_notice).color(warn));
        });
    });
}

/// List entry text: the section's `name`, or its target when it has none.
pub(super) fn keymap_label(keymap: &Keymap) -> String {
    if !keymap.name.is_empty() {
        return keymap.name.clone();
    }
    match &keymap.apps {
        AppFilter::All { .. } => i18n::t().config_apps_all.to_owned(),
        AppFilter::Names(names) => names.join(", "),
    }
}
