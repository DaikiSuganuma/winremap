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

//!
//! Split by screen (v1.1 plan D1): the address bar and file list in
//! `address_bar`, the navigation tree and breadcrumbs in `nav`, the read-only
//! panes in `view`, edit mode in `edit` / `edit_general`, the two help
//! popups in `help`, and the table and section primitives in `widgets`. This
//! file keeps the window's state and the frame-level flow.

mod address_bar;
mod edit;
mod edit_general;
mod help;
mod nav;
mod view;
mod widgets;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::i18n;
use crate::theme;
use address_bar::FileList;
use help::notation_help_ui;
use nav::breadcrumb_ui;
use view::{general_ui, keymap_ui};
use winremap::config::comments::ConfigComments;
use winremap::config::draft::{self, ConfigDraft};
use winremap::keymap::RemapTable;

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
    /// What the address bar's fixed widgets took the last time it was drawn,
    /// which is how much of the row the folder path may **not** use (see
    /// [`Self::address_row`]). Zero until the first frame has been drawn, and
    /// then the path simply gets the whole row for that one frame.
    header_reserved: f32,
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
                ui.label(egui::RichText::new(i18n::status_started(&super::started_at())).weak());
                ui.separator();
                ui.label(super::status());
            });
        });
}
