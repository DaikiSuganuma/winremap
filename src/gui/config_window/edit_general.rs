//! The general-settings pane in edit mode (v0.4 screen design §4.4):
//! `[macro]` and `[ime_indicator]` over the draft.

use eframe::egui;

use super::super::icons::Icon;
use super::edit::{Notation, notation_cell};
use super::help::key_names_help;
use super::view::recorded_macro_ui;
use super::widgets::section;
use crate::i18n;
use winremap::config::draft::ConfigDraft;
use winremap::ime_indicator_settings::{
    IndicatorSettings, MAX_INDICATOR_DURATION_MS, MAX_INDICATOR_SIZE, MIN_INDICATOR_DURATION_MS,
    MIN_INDICATOR_SIZE, parse_hex_color,
};
use winremap::keymap::MAX_MACRO_DELAY_MS;

/// General settings, editable (screen design §4.4): sliders for the numeric
/// ranges — always in-range by construction — checkboxes for the flags, and
/// key notation as text. The recorded macro stays the live, read-only box.
pub(super) fn general_edit_ui(ui: &mut egui::Ui, draft: &mut ConfigDraft) {
    let texts = i18n::t();
    ui.add_space(8.0);
    section(ui, Icon::Macro, texts.config_macro_section, "[macro]");
    // Show whichever spelling the file uses (ADR 0039); saving keeps it.
    let delay_key = if draft.uses_legacy_delay_key {
        "macro_delay_ms"
    } else {
        "delay_ms"
    };
    ui.horizontal(|ui| {
        ui.label(texts.config_macro_delay);
        ui.label(egui::RichText::new(delay_key).monospace().weak());
        let mut delay = draft.macro_delay.trim().parse::<u32>().unwrap_or(0);
        if ui
            .add(egui::Slider::new(&mut delay, 0..=MAX_MACRO_DELAY_MS))
            .changed()
        {
            draft.macro_delay = delay.to_string();
        }
    });
    for (label, key, value) in [
        (
            texts.config_macro_record_start,
            "record_start",
            &mut draft.macro_record.start,
        ),
        (
            texts.config_macro_record_stop,
            "record_stop",
            &mut draft.macro_record.stop,
        ),
        (
            texts.config_macro_record_play,
            "record_play",
            &mut draft.macro_record.play,
        ),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.label(egui::RichText::new(key).monospace().weak());
            notation_cell(ui, Notation::Chord, value, 160.0);
        });
    }
    ui.horizontal(|ui| {
        key_names_help(ui, "key-names-record");
    });

    // Live runtime state, not a file value — deliberately outside the
    // editable rows, exactly as in view mode.
    if crate::macro_record::recorded().is_some() {
        recorded_macro_ui(ui);
    }

    section(ui, Icon::Ime, texts.config_ime_indicator, "[ime_indicator]");
    let defaults = IndicatorSettings::default();
    let mut enabled = draft.ime.enabled.unwrap_or(defaults.enabled);
    if ui
        .checkbox(&mut enabled, texts.config_ime_enabled)
        .changed()
    {
        draft.ime.enabled = Some(enabled);
    }
    if enabled {
        for (label, key, value, min, max, fallback) in [
            (
                texts.config_ime_duration,
                "duration_ms",
                &mut draft.ime.duration_ms,
                MIN_INDICATOR_DURATION_MS,
                MAX_INDICATOR_DURATION_MS,
                defaults.duration_ms,
            ),
            (
                texts.config_ime_size,
                "size",
                &mut draft.ime.size,
                MIN_INDICATOR_SIZE,
                MAX_INDICATOR_SIZE,
                defaults.size,
            ),
            (
                texts.config_ime_opacity,
                "opacity",
                &mut draft.ime.opacity,
                0,
                255,
                u32::from(defaults.opacity),
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.label(egui::RichText::new(key).monospace().weak());
                let mut number = value.trim().parse::<u32>().unwrap_or(fallback);
                if ui.add(egui::Slider::new(&mut number, min..=max)).changed() {
                    *value = number.to_string();
                }
            });
        }
        let mut show_app = draft.ime.show_app_name.unwrap_or(defaults.show_app_name);
        if ui
            .checkbox(&mut show_app, texts.config_ime_show_app_name)
            .changed()
        {
            draft.ime.show_app_name = Some(show_app);
        }
        ui.horizontal(|ui| {
            ui.label(texts.config_ime_triggers);
            ui.label(egui::RichText::new("trigger_keys").monospace().weak());
            notation_cell(ui, Notation::ChordList, &mut draft.ime.trigger_keys, 220.0);
            key_names_help(ui, "key-names-triggers");
        });
    }

    // Outside the `enabled` block on purpose. The tint is independent of the
    // panel (ADR 0067) — it works with `enabled = false`, and acceptance M-6
    // is the item that checks exactly that. Nesting it here would tell the
    // reader the panel has to be switched on first, which is not true.
    let mut tint = draft
        .ime
        .change_cursor_color
        .unwrap_or(defaults.change_cursor_color);
    if ui
        .checkbox(&mut tint, texts.config_ime_change_cursor_color)
        .changed()
    {
        draft.ime.change_cursor_color = Some(tint);
    }
    if tint {
        ui.horizontal(|ui| {
            ui.label(texts.config_ime_cursor_color);
            ui.label(egui::RichText::new("cursor_color").monospace().weak());
            color_cell(ui, &mut draft.ime.cursor_color, defaults.cursor_color);
        });
    }
}

/// A `#rrggbb` edit cell: the box — warn-bordered when it does not parse —
/// followed by a swatch of the colour it resolves to. `#rrggbb` is the only
/// spelling the config takes ([`parse_hex_color`]), so a typo is worth showing
/// rather than silently becoming the default at load time. An empty box is
/// not a typo: it means the key is absent, which is the default colour.
fn color_cell(ui: &mut egui::Ui, text: &mut String, fallback: (u8, u8, u8)) {
    let trimmed = text.trim();
    let parsed = if trimmed.is_empty() {
        Some(fallback)
    } else {
        parse_hex_color(trimmed)
    };
    ui.scope(|ui| {
        if parsed.is_none() {
            let visuals = ui.visuals_mut();
            let warn = visuals.warn_fg_color;
            visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, warn);
            visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, warn);
        }
        ui.add(
            egui::TextEdit::singleline(text)
                .font(egui::TextStyle::Monospace)
                .desired_width(100.0),
        );
    });
    match parsed {
        // Painted rather than written as a block character: the bundled fonts
        // are not guaranteed to carry one, and a tofu box would read as a bug.
        Some((r, g, b)) => {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
            if ui.is_rect_visible(rect) {
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::same(2),
                    egui::Color32::from_rgb(r, g, b),
                );
            }
        }
        None => {
            let warn = ui.visuals().warn_fg_color;
            ui.label(
                egui::RichText::new(format!(
                    "\u{26a0} {}",
                    i18n::t().config_ime_cursor_color_hint
                ))
                .color(warn)
                .small(),
            );
        }
    }
}
