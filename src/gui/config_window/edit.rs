//! Edit mode (ADR 0049, v0.4 screen design §4.3 / §6 / §7): the footer band
//! for validation and confirmations, the body over the draft, the editable
//! keymap pane, and the key-notation cells with their live readings.

use std::time::{Duration, Instant};

use eframe::egui;

use super::super::icons::{self, Icon};
use super::edit_general::general_edit_ui;
use super::help::{key_names_help, notation_help_ui};
use super::nav::{breadcrumb_edit_ui, edit_list_ui};
use super::widgets::{comment_cell, own_note, section, table};
use super::{
    CAPTURE_DELAY, Capture, CaptureTarget, ConfigWindow, EditState, Notice, PendingAction,
    Selection,
};
use crate::i18n;
use crate::theme;
use crate::theme::{CELL_PAD, NOTE_GAP};
use winremap::config::comments::KeymapComments;
use winremap::config::draft::{KeymapDraft, RuleDraft};
use winremap::keymap::{
    KeyCombo, KeyParseError, parse_input_pattern, parse_key_combo, suggest_key_name,
};

impl ConfigWindow {
    /// The edit-mode footer band: validation results and confirmations
    /// (screen design §6.4/§7). Absent when there is nothing to say.
    pub(super) fn footer_ui(&mut self, ui: &mut egui::Ui) {
        let Some(edit) = self.edit.as_mut() else {
            return;
        };
        if edit.notice.is_none() && edit.issues.is_empty() {
            return;
        }
        let texts = i18n::t();
        let mut action = None;
        let notice = edit.notice.clone();
        egui::Panel::bottom("config-footer")
            .frame(theme::warn_band_frame(ui.visuals()))
            .show(ui, |ui| {
                ui.horizontal(|ui| match notice {
                    Some(Notice::ExternalChange) => {
                        ui.label(texts.config_external_changed);
                        if ui.button(texts.config_overwrite).clicked() {
                            action = Some(PendingAction::Overwrite);
                        }
                        if ui.button(texts.config_reread).clicked() {
                            action = Some(PendingAction::Reread);
                        }
                        if ui.button(texts.config_cancel).clicked() {
                            action = Some(PendingAction::DismissNotice);
                        }
                    }
                    Some(Notice::SaveFailed(reason)) => {
                        ui.label(i18n::save_failed(&reason));
                        if ui.button(texts.config_cancel).clicked() {
                            action = Some(PendingAction::DismissNotice);
                        }
                    }
                    Some(Notice::ConfirmClose) => {
                        ui.label(texts.config_close_confirm);
                        if ui.button(texts.config_close).clicked() {
                            action = Some(PendingAction::CloseDiscard);
                        }
                        if ui.button(texts.config_cancel).clicked() {
                            action = Some(PendingAction::DismissNotice);
                        }
                    }
                    None => {
                        let count = edit.issues.len();
                        if let Some(issue) = edit.issues.get(edit.issue_cursor) {
                            ui.label(i18n::issues_found(count, &issue.to_string()));
                        }
                        if count > 1 && ui.button(texts.config_next_issue).clicked() {
                            edit.issue_cursor = (edit.issue_cursor + 1) % count;
                        }
                    }
                });
            });
        self.queue(ui, action);
    }

    /// The window body in edit mode: everything shows the draft, never the
    /// live table (ADR 0049) — a tray reload changes what the hook does, not
    /// what is being edited.
    pub(super) fn edit_body_ui(&mut self, ui: &mut egui::Ui) {
        let selection = &mut self.selection;
        let comments = &self.comments;
        let Some(edit) = self.edit.as_mut() else {
            return;
        };
        let EditState { draft, capture, .. } = edit;

        egui::Panel::left("config-list")
            .default_size(220.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| edit_list_ui(ui, selection, draft));
            });

        let shown = *selection;
        if matches!(shown, Selection::Keymap(index) if index < draft.keymaps.len()) {
            egui::Panel::right("config-notation")
                .default_size(240.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, notation_help_ui);
                });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            breadcrumb_edit_ui(ui, draft, shown);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match shown {
                    Selection::Keymap(index) if index < draft.keymaps.len() => {
                        // Comments belong to the file the draft came from,
                        // so they are looked up by the keymap's origin.
                        let origin = draft.keymaps[index].origin;
                        let keymap_comments = origin.and_then(|i| comments.keymap(i));
                        keymap_edit_ui(
                            ui,
                            &mut draft.keymaps[index],
                            keymap_comments,
                            capture,
                            index,
                        );
                    }
                    _ => general_edit_ui(ui, draft),
                });
        });
    }
}

/// The keymap detail pane, editable (screen design §4.3). The section order
/// and headings are the viewer's; only the cells become inputs. Notation
/// fields carry their reading or problem beneath them (B3); the shared-input
/// column is still to come.
fn keymap_edit_ui(
    ui: &mut egui::Ui,
    keymap: &mut KeymapDraft,
    comments: Option<&KeymapComments>,
    capture: &mut Option<Capture>,
    index: usize,
) {
    let texts = i18n::t();
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(texts.config_field_name);
        ui.label(egui::RichText::new("name").monospace().weak());
        ui.add(
            egui::TextEdit::singleline(&mut keymap.name)
                .font(egui::TextStyle::Monospace)
                .desired_width(240.0),
        );
    });

    // The same rule the viewer applies, read from the draft's own strings.
    let all_apps = keymap.application.iter().any(|app| app == "*");

    section(ui, Icon::Apps, texts.config_field_apps, "application");
    edit_string_table(ui, "apps-edit", &mut keymap.application);
    ui.add_space(f32::from(CELL_PAD));
    // With "*" on the list every application already matches, so adding one,
    // capturing one, or asking for "*" again all say nothing — the buttons go
    // rather than sit there inert (owner decision 2026-07-26). Removing the
    // "*" row brings them back.
    if !all_apps {
        ui.horizontal(|ui| {
            if icons::button(ui, Icon::Plus, texts.config_add_app).clicked() {
                keymap.application.push(String::new());
            }
            capture_button(
                ui,
                &mut keymap.application,
                capture,
                index,
                CaptureTarget::Apps,
            );
            if icons::button(ui, Icon::Apps, texts.config_target_all_apps).clicked() {
                // Replaces rather than appends: "*" beside exe names is a
                // validation error, not a wider list (config-spec §3).
                keymap.application = vec!["*".to_owned()];
            }
        });
    }
    ui.add_space(NOTE_GAP);
    own_note(ui, texts.config_apps_case_note);

    // Exclusions only mean anything against "*".
    if all_apps {
        section(ui, Icon::Exclude, texts.config_field_exclude, "exclude");
        edit_string_table(ui, "excludes-edit", &mut keymap.exclude);
        ui.add_space(f32::from(CELL_PAD));
        ui.horizontal(|ui| {
            if icons::button(ui, Icon::Plus, texts.config_add_app).clicked() {
                keymap.exclude.push(String::new());
            }
            // Same capture, other list: naming the app in front is how a
            // keymap learns an exe name, whichever list it is going on
            // (owner decision 2026-07-26).
            capture_button(
                ui,
                &mut keymap.exclude,
                capture,
                index,
                CaptureTarget::Exclude,
            );
        });
    }

    section(ui, Icon::Rules, texts.config_rules, "[keymap.remap]");
    edit_rules_table(ui, &mut keymap.rules, comments);
    ui.add_space(f32::from(CELL_PAD));
    ui.horizontal(|ui| {
        if icons::button(ui, Icon::Plus, texts.config_add_rule).clicked() {
            keymap.rules.push(RuleDraft::default());
        }
        key_names_help(ui, "key-names-rules");
    });
}

/// What a key-notation field's feedback line says (screen design §6.1–6.2):
/// the reading of a valid value, the reason for an invalid one.
enum NotationCheck {
    Valid(String),
    Invalid(String),
}

/// Which notation a field holds, and so how it is parsed and read back.
#[derive(Clone, Copy)]
pub(super) enum Notation {
    /// A rule input: a chord or a two-stroke sequence.
    Input,
    /// A rule output: a chord, or a comma-separated macro.
    Output,
    /// A single chord (the recording keys).
    Chord,
    /// Comma-separated chords (the IME trigger keys).
    ChordList,
}

fn check_notation(kind: Notation, text: &str) -> Option<NotationCheck> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        // A row still being typed; the save-time validation still gates.
        return None;
    }
    // The same keyboard the config is compiled against, so the box turns red
    // for exactly the rules a save would reject (ADR 0063).
    let layout = crate::layout::current();
    let rendered = match kind {
        Notation::Input => {
            parse_input_pattern(trimmed, &layout).map(|pattern| i18n::input_human(&pattern))
        }
        Notation::Chord => parse_key_combo(trimmed, &layout).map(|combo| i18n::combo_human(&combo)),
        Notation::Output => parse_combo_list(trimmed).map(|combos| i18n::output_human(&combos)),
        Notation::ChordList => parse_combo_list(trimmed).map(|combos| {
            combos
                .iter()
                .map(i18n::combo_human)
                .collect::<Vec<_>>()
                .join(" / ")
        }),
    };
    Some(match rendered {
        Ok(reading) => NotationCheck::Valid(reading),
        Err(error) => NotationCheck::Invalid(notation_error_text(&error)),
    })
}

fn parse_combo_list(text: &str) -> Result<Vec<KeyCombo>, KeyParseError> {
    let layout = crate::layout::current();
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| parse_key_combo(part, &layout))
        .collect()
}

/// The reason line under an invalid field: the parser's technical English,
/// except an unknown key name with a near miss, which earns the friendlier
/// "did you mean" (screen design §6.2).
fn notation_error_text(error: &KeyParseError) -> String {
    if let KeyParseError::UnknownKey(name) = error
        && let Some(suggest) = suggest_key_name(name)
    {
        return i18n::unknown_key_suggestion(name, suggest);
    }
    error.to_string()
}

/// A key-notation edit cell: the box — warn-bordered when invalid — with its
/// reading (ⓘ) or the problem (⚠) directly beneath (screen design §6).
/// The check runs against the text as of this frame's start; one frame of
/// lag, invisible in use.
pub(super) fn notation_cell(ui: &mut egui::Ui, kind: Notation, text: &mut String, width: f32) {
    let check = check_notation(kind, text);
    let invalid = matches!(check, Some(NotationCheck::Invalid(_)));
    ui.vertical(|ui| {
        ui.scope(|ui| {
            if invalid {
                let visuals = ui.visuals_mut();
                let warn = visuals.warn_fg_color;
                visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, warn);
                visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, warn);
            }
            ui.add(
                egui::TextEdit::singleline(text)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(width),
            );
        });
        match check {
            Some(NotationCheck::Valid(reading)) => {
                ui.label(
                    egui::RichText::new(format!("\u{24d8} {reading}"))
                        .weak()
                        .small(),
                );
            }
            Some(NotationCheck::Invalid(reason)) => {
                let warn = ui.visuals().warn_fg_color;
                ui.label(
                    egui::RichText::new(format!("\u{26a0} {reason}"))
                        .color(warn)
                        .small(),
                );
            }
            None => {}
        }
    });
}

/// The "capture the foreground app" button (B4, screen design §6.3).
/// Pressing it starts a countdown — the press itself puts the settings
/// window in the foreground, which is exactly the wrong answer — and when
/// it fires, the exe in front lands in `list`.
fn capture_button(
    ui: &mut egui::Ui,
    list: &mut Vec<String>,
    capture: &mut Option<Capture>,
    index: usize,
    target: CaptureTarget,
) {
    let texts = i18n::t();
    match capture {
        Some(pending) if pending.keymap == index && pending.target == target => {
            let now = Instant::now();
            if pending.deadline <= now {
                // Asked directly rather than read from the hook's cache: that
                // cache is a thread_local belonging to the message loop, and
                // this is the GUI thread. Refreshing it from here wrote a
                // copy nothing else reads (ADR 0065). The call is the safe
                // public surface of `window` — no unsafe enters the GUI
                // (invariant 3).
                let exe = crate::window::query_foreground_exe();
                if !exe.is_empty() && !list.iter().any(|app| app.eq_ignore_ascii_case(&exe)) {
                    list.push(exe);
                }
                *capture = None;
            } else {
                let seconds = (pending.deadline - now).as_secs_f32().ceil() as u64;
                ui.add_enabled_ui(false, |ui| {
                    let _ = icons::button(ui, Icon::Hourglass, &i18n::capture_countdown(seconds));
                });
                // Keep painting so the count visibly ticks and the deadline
                // fires without waiting for other input.
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }
        }
        _ => {
            if icons::button(ui, Icon::Apps, texts.config_capture_app).clicked() {
                crate::gui::log::action(texts.config_capture_app);
                *capture = Some(Capture {
                    keymap: index,
                    target,
                    deadline: Instant::now() + CAPTURE_DELAY,
                });
            }
        }
    }
}

/// One editable exe name per row with a per-row delete, in `table()`'s
/// shape so view and edit read alike.
fn edit_string_table(ui: &mut egui::Ui, id: &str, values: &mut Vec<String>) {
    let texts = i18n::t();
    let columns = [texts.config_column_app, ""];
    let mut remove = None;
    table(ui, id, &columns, 220.0, |ui| {
        for (index, value) in values.iter_mut().enumerate() {
            ui.add(
                egui::TextEdit::singleline(value)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(260.0),
            );
            if icons::icon_button(ui, Icon::Close).clicked() {
                remove = Some(index);
            }
            ui.end_row();
        }
    });
    if let Some(index) = remove {
        values.remove(index);
    }
}

/// Editable remap rules: raw spellings on both sides (ADR 0049 decision 5 —
/// what the user wrote is what they edit), macros as comma-separated output.
/// Each side carries its live reading or problem beneath it (B3), and the
/// comment written next to the rule rides along read-only.
fn edit_rules_table(
    ui: &mut egui::Ui,
    rules: &mut Vec<RuleDraft>,
    comments: Option<&KeymapComments>,
) {
    let texts = i18n::t();
    let columns = [
        texts.config_rule_input,
        texts.config_rule_output,
        texts.config_rule_comment,
        "",
    ];
    let mut remove = None;
    table(ui, "rules-edit", &columns, 120.0, |ui| {
        for (index, rule) in rules.iter_mut().enumerate() {
            notation_cell(ui, Notation::Input, &mut rule.input, 160.0);
            notation_cell(ui, Notation::Output, &mut rule.output, 220.0);
            // Comments are keyed by the canonical form; an input mid-typing
            // simply finds none and the cell stays empty until it parses.
            let comment =
                winremap::config::comments::canonical_input(&rule.input, &crate::layout::current())
                    .and_then(|canonical| comments.and_then(|c| c.rule(&canonical)))
                    .unwrap_or_default();
            comment_cell(ui, comment);
            if icons::icon_button(ui, Icon::Close).clicked() {
                remove = Some(index);
            }
            ui.end_row();
        }
    });
    if let Some(index) = remove {
        rules.remove(index);
    }
}
