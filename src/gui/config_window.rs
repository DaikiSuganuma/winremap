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

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;

use super::icons::{self, Icon};
// Every size and colour in this window comes from `theme`, so they can be
// read and adjusted in one place.
use crate::i18n;
use crate::theme;
use crate::theme::{CELL_PAD, EDGE_PAD, NOTE_GAP, SECTION_GAP, SECTION_TEXT};
use winremap::config::comments::{ConfigComments, KeymapComments};
use winremap::config::draft::{self, ConfigDraft, KeymapDraft, RuleDraft};
use winremap::ime_indicator_settings::{
    IndicatorSettings, MAX_INDICATOR_DURATION_MS, MAX_INDICATOR_SIZE, MIN_INDICATOR_DURATION_MS,
    MIN_INDICATOR_SIZE, parse_hex_color,
};
use winremap::keymap::{
    AppFilter, KeyCombo, KeyParseError, Keymap, MAX_MACRO_DELAY_MS, Output, RemapTable,
    SPECIAL_KEY_NAMES, combo_notation, key_name, parse_input_pattern, parse_key_combo,
    suggest_key_name, vk_display_name,
};

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
    /// The address bar's view of the config folder; see [`FileList`].
    files: FileList,
    /// `Some` = edit mode (ADR 0049). The window shows the draft, never the
    /// live table, until Save or Revert ends it.
    edit: Option<EditState>,
    /// A button press from the previous frame; see [`PendingAction`].
    pending: Option<PendingAction>,
}

/// Everything edit mode holds: the draft being edited, the pristine copy the
/// save diffs against (ADR 0036 — untouched lines must stay untouched), and
/// the file identity for the external-change check.
struct EditState {
    original: ConfigDraft,
    draft: ConfigDraft,
    stamp: Option<draft::FileStamp>,
    /// Validation results of the last failed save attempt (screen design
    /// §6.4).
    issues: Vec<winremap::config::Issue>,
    issue_cursor: usize,
    notice: Option<Notice>,
    /// A running foreground capture (B4): which keymap asked, and when the
    /// countdown fires.
    capture: Option<Capture>,
}

struct Capture {
    keymap: usize,
    /// Which of the keymap's two lists the captured exe lands on — both carry
    /// the button, and only the one that was pressed may count down.
    target: CaptureTarget,
    deadline: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptureTarget {
    Apps,
    Exclude,
}

/// The grace the capture gives for bringing the target app to the front —
/// pressing the button makes the settings window the foreground app, which
/// is exactly the wrong answer (screen design §6.3).
const CAPTURE_DELAY: Duration = Duration::from_secs(3);

/// What the footer band is currently asking or reporting (screen design
/// §6.4/§7). One at a time: each replaces the last.
#[derive(Clone)]
enum Notice {
    ExternalChange,
    SaveFailed(String),
    ConfirmClose,
}

/// What a header or footer button asked for. Recorded rather than performed:
/// every one of these changes which panels the window has, and doing that
/// halfway through a frame leaves egui's second layout pass disagreeing with
/// the first about what sits where — which it reports by outlining the
/// window in red for that frame (owner feedback 2026-07-26, acceptance C-11).
/// Applied at the top of the next frame instead, where every pass sees the
/// same window.
enum PendingAction {
    Edit,
    Save,
    Revert,
    Overwrite,
    Reread,
    CloseDiscard,
    /// Dismiss the footer's question without acting on it.
    DismissNotice,
}

/// How stale the folder listing and the change marks may get when the
/// notify watch (ADR 0051) could not start and polling is the fallback.
/// Short enough that an external save shows up while the window is open,
/// long enough that painting stays free of disk access.
const FILE_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The `*.toml` files beside the active config, with their change marks
/// (v0.4 screen design §2.2–2.3).
#[derive(Default)]
struct FileList {
    checked: Option<Instant>,
    entries: Vec<FileEntry>,
    /// The stamp each file had when first listed. ● on another file means
    /// "differs from this"; on the active file it means "differs from what
    /// is loaded" — the state the removed timestamps used to convey.
    first_seen: HashMap<String, draft::FileStamp>,
    /// The loaded stamp the marks were computed against, so a reload —
    /// which produces no folder event — still clears the active file's ●.
    loaded_seen: Option<draft::FileStamp>,
}

struct FileEntry {
    name: String,
    changed: bool,
}

impl FileList {
    fn refresh(&mut self, path: &Path) {
        let cue = super::watch::take_dirty();
        let loaded = super::loaded_stamp();
        if super::watch::active() {
            // Event-driven (ADR 0051): the folder is looked at again on a
            // watch cue, on a reload, or on the first frame — otherwise
            // painting never touches the disk at all.
            if !cue && loaded == self.loaded_seen && self.checked.is_some() {
                return;
            }
        } else if !cue
            && let Some(checked) = self.checked
            && checked.elapsed() < FILE_POLL_INTERVAL
        {
            return;
        }
        self.checked = Some(Instant::now());
        self.loaded_seen = loaded;
        self.entries.clear();
        let Some(folder) = path.parent() else { return };
        let Ok(dir) = std::fs::read_dir(folder) else {
            return;
        };
        let mut names: Vec<String> = dir
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.to_ascii_lowercase().ends_with(".toml").then_some(name)
            })
            .collect();
        names.sort_by_key(|name| name.to_ascii_lowercase());
        let active = file_name(path);
        for name in names {
            let stamp = draft::stamp(&folder.join(&name)).ok();
            let changed = if name.eq_ignore_ascii_case(&active) {
                matches!((loaded, stamp), (Some(loaded), Some(stamp)) if loaded != stamp)
            } else {
                matches!(
                    (self.first_seen.get(&name), stamp),
                    (Some(first), Some(stamp)) if *first != stamp
                )
            };
            if let Some(stamp) = stamp {
                self.first_seen.entry(name.clone()).or_insert(stamp);
            }
            self.entries.push(FileEntry { name, changed });
        }
    }

    fn is_changed(&self, name: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.name == name && entry.changed)
    }
}

/// The file's name for display; the path up to it is the folder's job.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl ConfigWindow {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let texts = i18n::t();
        let path = super::active_config_path();
        // A snapshot for the whole frame: the hook may swap the table at any
        // moment (ADR 0003), and the list and the detail pane have to agree.
        let table = crate::hook::REMAP_TABLE.load_full();
        self.sync_comments(table.as_ref(), &path);
        // Before anything is laid out, so this frame is built once from one
        // state (see `PendingAction`).
        if let Some(action) = self.pending.take() {
            self.apply(action, &path);
        }

        self.header_ui(ui, &path);
        statusbar_ui(ui);
        self.footer_ui(ui);

        if self.edit.is_some() {
            self.edit_body_ui(ui);
            return;
        }

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
            breadcrumb_ui(ui, &table, self.selection);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.selection {
                    Selection::General => general_ui(ui, &table, &self.comments),
                    // The table can shrink under us on a reload; fall back to
                    // the general page rather than panicking on the index.
                    Selection::Keymap(index) => match table.keymaps.get(index) {
                        Some(keymap) => {
                            keymap_ui(ui, &table, index, keymap, self.comments.keymap(index))
                        }
                        None => general_ui(ui, &table, &self.comments),
                    },
                });
        });
    }

    /// The Explorer-style address bar (v0.4 screen design §2): the folder,
    /// the file as a dropdown over the folder's `*.toml`s, reload, and — at
    /// the right end — the edit button (owner decision 2026-07-22). While
    /// editing, the band takes the edit colour, the dropdown goes inert, the
    /// reload button hides, and the right end turns into Save / Revert
    /// (§2.4) — the same spot, so the hand does not travel.
    fn header_ui(&mut self, ui: &mut egui::Ui, path: &Path) {
        let texts = i18n::t();
        let editing = self.edit.is_some();
        let frame = if editing {
            theme::edit_mode_frame(ui.visuals())
        } else {
            theme::chrome_frame(ui.visuals())
        };
        let mut action = None;
        egui::Panel::top("config-header")
            .frame(frame)
            .show(ui, |ui| {
                self.files.refresh(path);
                let active = file_name(path);
                // The row's height is fixed *before* laying anything out.
                // `ui.horizontal` pins its centring axis to the default
                // interact height, so short labels ride high and anything
                // taller sags below it — `set_height` cannot move that axis
                // (owner feedback 2026-07-22). The height comes from a real
                // galley of the button label: the labels are Japanese, and
                // the JP fallback face stands taller than the Latin font's
                // nominal row height.
                let label = if editing {
                    texts.config_save
                } else {
                    texts.config_edit
                };
                let font = egui::TextStyle::Button.resolve(ui.style());
                let text_height = ui
                    .painter()
                    .layout_no_wrap(label.to_owned(), font, egui::Color32::PLACEHOLDER)
                    .size()
                    .y;
                let row_height =
                    text_height.max(theme::button_icon_size(ui)) + 2.0 * theme::BUTTON_PADDING;
                let row_size = egui::vec2(ui.available_width(), row_height);
                let layout = egui::Layout::left_to_right(egui::Align::Center);
                ui.allocate_ui_with_layout(row_size, layout, |ui| {
                    // ComboBox lays itself out on an interact_size-high axis
                    // of its own; with the app's button padding it outgrows
                    // that axis, sags below centre and drags the row taller
                    // (verified with a headless probe). Matching the
                    // interact height to the row pins every widget to one
                    // axis.
                    ui.spacing_mut().interact_size.y = row_height;
                    icons::show(ui, Icon::Folder, theme::body_icon_size(ui));
                    let folder = path
                        .parent()
                        .map(|folder| folder.display().to_string())
                        .unwrap_or_default();
                    ui.label(egui::RichText::new(folder).monospace().weak());
                    ui.label(egui::RichText::new("›").weak());
                    if editing {
                        // Switching files mid-edit would orphan the draft.
                        ui.add_enabled_ui(false, |ui| {
                            egui::ComboBox::from_id_salt("config-file-switch")
                                .selected_text(egui::RichText::new(active.clone()).monospace())
                                .show_ui(ui, |_| {});
                        })
                        .response
                        .on_hover_text(texts.config_switch_locked);
                    } else {
                        let changed = self.files.is_changed(&active);
                        let shown = if changed {
                            format!("{active} ●")
                        } else {
                            active.clone()
                        };
                        let combo = egui::ComboBox::from_id_salt("config-file-switch")
                            .selected_text(egui::RichText::new(shown).monospace())
                            .show_ui(ui, |ui| file_menu_ui(ui, &self.files, path, &active));
                        if changed {
                            combo.response.on_hover_text(texts.config_file_changed);
                        }
                        ui.add_space(4.0);
                        if icons::icon_button(ui, Icon::Reload)
                            .on_hover_text(texts.menu_reload)
                            .clicked()
                        {
                            super::log::action(texts.menu_reload);
                            super::request_reload();
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if editing {
                            // Right-to-left: Revert lands rightmost, Save to
                            // its left — reading order Save, Revert (§2.4).
                            if icons::button(ui, Icon::Revert, texts.config_revert).clicked() {
                                action = Some(PendingAction::Revert);
                            }
                            if icons::button(ui, Icon::Floppy, texts.config_save).clicked() {
                                action = Some(PendingAction::Save);
                            }
                        } else if icons::button(ui, Icon::Pencil, texts.config_edit).clicked() {
                            action = Some(PendingAction::Edit);
                        }
                    });
                });
            });
        self.queue(ui, action);
    }

    /// Holds a button press until the next frame, and makes sure there is
    /// one — the click already woke the loop, but the frame that acts on it
    /// must not depend on further input arriving.
    fn queue(&mut self, ui: &egui::Ui, action: Option<PendingAction>) {
        if action.is_some() {
            self.pending = action;
            ui.ctx().request_repaint();
        }
    }

    /// Performs what a button asked for last frame; see [`PendingAction`].
    fn apply(&mut self, action: PendingAction, path: &Path) {
        match action {
            PendingAction::Edit | PendingAction::Reread => self.start_edit(path),
            PendingAction::Save => self.save(path, false),
            PendingAction::Overwrite => self.save(path, true),
            PendingAction::Revert => self.revert(),
            PendingAction::CloseDiscard => {
                self.edit = None;
                super::hide_config();
            }
            PendingAction::DismissNotice => {
                if let Some(edit) = self.edit.as_mut() {
                    edit.notice = None;
                }
            }
        }
    }

    /// Reads the file into a fresh draft and enters edit mode (ADR 0049
    /// decision 2: the file, not the live table, is the edit's origin).
    fn start_edit(&mut self, path: &Path) {
        match draft::read(path) {
            Ok((parsed, stamp)) => {
                super::log::action(i18n::t().config_edit);
                self.edit = Some(EditState {
                    original: parsed.clone(),
                    draft: parsed,
                    stamp: Some(stamp),
                    issues: Vec::new(),
                    issue_cursor: 0,
                    notice: None,
                    capture: None,
                });
            }
            Err(error) => super::set_status(&i18n::edit_cannot_start(&error.to_string())),
        }
    }

    fn revert(&mut self) {
        super::log::action(i18n::t().config_revert);
        self.edit = None;
    }

    /// The save transaction (design doc §4): stamp check, re-read, apply the
    /// draft's diff, validate with the same parser the CLI and tray use,
    /// write atomically, reload. Any failure keeps edit mode — the draft is
    /// never the casualty.
    fn save(&mut self, path: &Path, force: bool) {
        let Some(edit) = self.edit.as_mut() else {
            return;
        };
        edit.issues.clear();
        edit.issue_cursor = 0;
        edit.notice = None;
        if !force
            && let (Some(opened), Ok(now)) = (edit.stamp, draft::stamp(path))
            && opened != now
        {
            // Never silently overwrite an outside edit (design doc §6.2).
            edit.notice = Some(Notice::ExternalChange);
            return;
        }
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                edit.notice = Some(Notice::SaveFailed(error.to_string()));
                return;
            }
        };
        let text = match draft::apply(&source, &edit.original, &edit.draft) {
            Ok(text) => text,
            Err(error) => {
                edit.notice = Some(Notice::SaveFailed(error.to_string()));
                return;
            }
        };
        match winremap::config::parse_str(&text, &crate::layout::current()) {
            Ok(_) => {}
            Err(winremap::config::ConfigError::Invalid(issues)) => {
                edit.issues = issues;
                return;
            }
            Err(other) => {
                edit.notice = Some(Notice::SaveFailed(other.to_string()));
                return;
            }
        }
        if let Err(error) = draft::write_atomic(path, &text) {
            edit.notice = Some(Notice::SaveFailed(error.to_string()));
            return;
        }
        super::log::action(i18n::t().config_save);
        super::set_status(i18n::t().status_saved);
        self.edit = None;
        // The tray reloads it back in: read, atomic swap, stamp (ADR 0003).
        super::request_reload();
    }

    /// Called when the window is asked to close. An unsaved draft turns the
    /// close into a footer confirmation instead (screen design §7.3); a
    /// clean one is discarded silently.
    pub fn intercept_close(&mut self) -> bool {
        match self.edit.as_mut() {
            Some(edit) if edit.draft != edit.original => {
                edit.notice = Some(Notice::ConfirmClose);
                true
            }
            _ => {
                self.edit = None;
                false
            }
        }
    }

    /// The edit-mode footer band: validation results and confirmations
    /// (screen design §6.4/§7). Absent when there is nothing to say.
    fn footer_ui(&mut self, ui: &mut egui::Ui) {
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
    fn edit_body_ui(&mut self, ui: &mut egui::Ui) {
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
        self.comments = winremap::config::comments::read(path, &crate::layout::current());
    }

    /// The navigation tree (v0.4 screen design §3): icons, an indent for the
    /// keymaps under their heading, no expand/collapse.
    fn list_ui(&mut self, ui: &mut egui::Ui, table: &RemapTable) {
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

/// The address bar's dropdown: every `*.toml` beside the active file — ✓ on
/// the one in use, ● on any that changed on disk — plus the file actions,
/// which live here rather than as more header icons (owner decision
/// 2026-07-22).
fn file_menu_ui(ui: &mut egui::Ui, files: &FileList, path: &Path, active: &str) {
    let texts = i18n::t();
    for entry in &files.entries {
        let current = entry.name == active;
        let label = format!(
            "{}{}{}",
            if current { "✓ " } else { "  " },
            entry.name,
            if entry.changed { " ●" } else { "" }
        );
        let row = ui.selectable_label(current, egui::RichText::new(label).monospace());
        let row = if entry.changed {
            row.on_hover_text(texts.config_file_changed)
        } else {
            row
        };
        if row.clicked()
            && !current
            && let Some(folder) = path.parent()
        {
            // Switching = swap the path, then the ordinary reload path does
            // the loading (ADR 0050). A file that fails to load behaves like
            // any failed reload: the live table stays. The choice also
            // outlives the run, which is what `choose_` means here (ADR 0077).
            super::log::action(&i18n::action_switch_file(&entry.name));
            super::choose_config_path(folder.join(&entry.name));
            super::request_reload();
        }
    }
    ui.separator();
    // Breathing room around the action links (owner decision 2026-07-22):
    // they read as a menu section of their own, not as more list rows.
    ui.add_space(f32::from(CELL_PAD));
    ui.horizontal(|ui| {
        ui.add_space(NOTE_GAP);
        if icons::link(ui, Icon::External, texts.config_window_open_in_editor) {
            open_in_default_editor(path);
        }
        ui.add_space(NOTE_GAP);
    });
    ui.add_space(f32::from(CELL_PAD));
    ui.horizontal(|ui| {
        ui.add_space(NOTE_GAP);
        if icons::link(ui, Icon::Folder, texts.config_open_folder)
            && let Some(folder) = path.parent()
        {
            super::log::action(texts.config_open_folder);
            if !super::win32::open_folder(folder) {
                crate::notify::error(&i18n::open_folder_failed(&folder.display().to_string()));
            }
        }
        ui.add_space(NOTE_GAP);
    });
    ui.add_space(f32::from(CELL_PAD));
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
fn breadcrumb_ui(ui: &mut egui::Ui, table: &RemapTable, selection: Selection) {
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
fn edit_list_ui(ui: &mut egui::Ui, selection: &mut Selection, draft: &mut ConfigDraft) {
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
fn breadcrumb_edit_ui(ui: &mut egui::Ui, draft: &ConfigDraft, selection: Selection) {
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
enum Notation {
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
fn notation_cell(ui: &mut egui::Ui, kind: Notation, text: &mut String, width: f32) {
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

/// The key-name reference behind a `?` button (screen design §6.5),
/// generated from the parser's own list so the two cannot drift.
fn key_names_help(ui: &mut egui::Ui, salt: &str) {
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
                super::log::action(texts.config_capture_app);
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

/// General settings, editable (screen design §4.4): sliders for the numeric
/// ranges — always in-range by construction — checkboxes for the flags, and
/// key notation as text. The recorded macro stays the live, read-only box.
fn general_edit_ui(ui: &mut egui::Ui, draft: &mut ConfigDraft) {
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

/// The permanent bottom band (v0.4 screen design §5): the version — its one
/// home in this window (owner decision 2026-07-22) — when this run started,
/// and the last thing that happened.
fn statusbar_ui(ui: &mut egui::Ui) {
    let texts = i18n::t();
    egui::Panel::bottom("config-statusbar")
        .frame(theme::chrome_frame(ui.visuals()))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} v{}",
                        texts.app_name,
                        env!("CARGO_PKG_VERSION")
                    ))
                    .weak(),
                );
                ui.separator();
                ui.label(egui::RichText::new(i18n::status_started(super::started_at())).weak());
                ui.separator();
                ui.label(super::status());
            });
        });
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

fn keymap_ui(
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

/// Grid spacing is the gap *between* cells, so it is twice the padding each
/// cell gets. Columns get the wider gap: adjacent values need more to read
/// apart than stacked rows do.
fn cell_spacing() -> egui::Vec2 {
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
fn table(
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
fn comment_cell(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(text).wrap());
}

/// Sections are separated by whitespace alone: the pane had collected enough
/// horizontal rules — table borders, dividers — that they stopped reading as
/// structure and started reading as clutter (owner decision 2026-07-21). The
/// gap is doubled to carry the break the line used to.
fn section(ui: &mut egui::Ui, icon: Icon, title: &str, key: &str) {
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
fn own_note(ui: &mut egui::Ui, text: &str) {
    ui.label(format!("{} {text}", i18n::t().note_marker));
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

/// What `C-` and friends mean. Lives in its own pane beside the rules, which
/// are unreadable without it for anyone who did not write them, and always
/// open — a legend behind a disclosure triangle helps nobody.
///
/// No border or header here: it is a legend, not data, and giving it the same
/// weight as the rule table would make the pane compete with it.
fn notation_help_ui(ui: &mut egui::Ui) {
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
fn recorded_macro_ui(ui: &mut egui::Ui) {
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
