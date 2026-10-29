//! The Explorer-style address bar (v0.4 screen design §2): the folder, the
//! file dropdown over the folder's `*.toml`s with their change marks, reload,
//! and the edit / save / revert buttons at the right end.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use eframe::egui;

use super::super::icons::{self, Icon};
use super::{ConfigWindow, PendingAction};
use crate::i18n;
use crate::theme;
use crate::theme::{CELL_PAD, NOTE_GAP};
use winremap::config::draft;

/// How stale the folder listing and the change marks may get when the
/// notify watch (ADR 0051) could not start and polling is the fallback.
/// Short enough that an external save shows up while the window is open,
/// long enough that painting stays free of disk access.
const FILE_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The `*.toml` files beside the active config, with their change marks
/// (v0.4 screen design §2.2–2.3).
#[derive(Default)]
pub(super) struct FileList {
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
        let cue = crate::gui::watch::take_dirty();
        let loaded = crate::gui::loaded_stamp();
        if crate::gui::watch::active() {
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

/// The address bar's file dropdown. Named once: the same id has to reach both
/// the enabled and the edit-mode-disabled dropdown, or switching into edit
/// mode would look like a different widget to egui.
const SWITCH_SALT: &str = "config-file-switch";

/// What one line of the address bar's own font takes up.
fn monospace_width(ui: &egui::Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER)
        .size()
        .x
}

/// Shortens a folder path from the middle until it fits `budget`, Explorer
/// style: the drive stays, the deepest folders stay, and everything dropped in
/// between becomes a single `…`.
///
/// **The two ends are the two things a person actually reads** — which drive
/// this is, and which folder it is. The middle is what a config folder buried
/// under `AppData\Local\Temp\...` has most of, and it is what says least.
///
/// `width` measures a candidate; taking it as an argument keeps this pure, so
/// the shortening can be tested without a font.
fn elide_path(path: &str, budget: f32, width: impl Fn(&str) -> f32) -> String {
    if width(path) <= budget {
        return path.to_owned();
    }
    let parts: Vec<&str> = path.split('\\').collect();
    // Longest first, so the answer is the most that fits rather than the
    // least. Two parts or fewer have no middle to drop.
    for keep in (1..parts.len().saturating_sub(1)).rev() {
        let candidate = format!(
            "{}\\…\\{}",
            parts[0],
            parts[parts.len() - keep..].join("\\")
        );
        if width(&candidate) <= budget {
            return candidate;
        }
    }
    // Not even the deepest folder fits beside the drive, so the drive goes
    // too and the folder's name is eaten from the front — its tail is the
    // part that tells two sibling folders apart.
    let tail = parts.last().copied().unwrap_or(path);
    let mut from = 0;
    while from < tail.len() {
        let candidate = format!("…{}", &tail[from..]);
        if width(&candidate) <= budget {
            return candidate;
        }
        // On a char boundary: a path can hold any character at all.
        from += 1;
        while from < tail.len() && !tail.is_char_boundary(from) {
            from += 1;
        }
    }
    "…".to_owned()
}

/// The file's name for display; the path up to it is the folder's job.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl ConfigWindow {
    /// The Explorer-style address bar (v0.4 screen design §2): the folder,
    /// the file as a dropdown over the folder's `*.toml`s, reload, and — at
    /// the right end — the edit button (owner decision 2026-07-22). While
    /// editing, the band takes the edit colour, the dropdown goes inert, the
    /// reload button hides, and the right end turns into Save / Revert
    /// (§2.4) — the same spot, so the hand does not travel.
    ///
    /// **The folder path is the one thing in this row with no bound on its
    /// length**, and egui's horizontal layout does not push back: a deep
    /// folder simply carried the dropdown and the edit button past the right
    /// edge of the window, where they could not be clicked at all (owner
    /// report 2026-08-16). So the path is shortened to whatever the row has
    /// left over — and how much that is is **measured by the row itself**,
    /// see [`Self::address_row`].
    pub(super) fn header_ui(&mut self, ui: &mut egui::Ui, path: &Path) {
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
                    let folder = path
                        .parent()
                        .map(|folder| folder.display().to_string())
                        .unwrap_or_default();
                    let budget = ui.available_width() - self.header_reserved;
                    let shown = elide_path(&folder, budget, |text| monospace_width(ui, text));
                    // Whatever the shortened path did not need, so the
                    // right-hand buttons still sit at the right edge (§2.4).
                    let gap = (budget - monospace_width(ui, &shown)).max(0.0);
                    self.header_reserved =
                        self.address_row(ui, path, &shown, &folder, gap, &mut action);
                });
            });
        self.queue(ui, action);
    }

    /// The address bar's widgets, and **what everything except the path took**
    /// — which is what the next frame gives the path to fit in.
    ///
    /// Measured here rather than predicted, because a prediction is a second
    /// description of this row: it would say "an icon, a chevron, a dropdown,
    /// two buttons and six gaps" somewhere else, and part company with the row
    /// the first time a button was added. What the row itself did cannot be
    /// out of date — only one frame old, and a window opening or being resized
    /// settles in the frame after it moves.
    ///
    /// `shown` is the path as it will be drawn and `full` as it is; they
    /// differ when it had to be shortened, and then the whole thing is a
    /// tooltip away.
    fn address_row(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        shown: &str,
        full: &str,
        gap: f32,
        action: &mut Option<PendingAction>,
    ) -> f32 {
        let texts = i18n::t();
        let editing = self.edit.is_some();
        let active = file_name(path);
        let left = ui.cursor().min.x;
        icons::show(ui, Icon::Folder, theme::body_icon_size(ui));
        let label = ui.label(egui::RichText::new(shown).monospace().weak());
        let path_width = label.rect.width();
        if shown != full {
            label.on_hover_text(full);
        }
        ui.label(egui::RichText::new("›").weak());
        if editing {
            // Switching files mid-edit would orphan the draft.
            ui.add_enabled_ui(false, |ui| {
                egui::ComboBox::from_id_salt(SWITCH_SALT)
                    .selected_text(egui::RichText::new(active.clone()).monospace())
                    .show_ui(ui, |_| {});
            })
            .response
            .on_hover_text(texts.config_switch_locked);
        } else {
            let changed = self.files.is_changed(&active);
            let selected = if changed {
                format!("{active} ●")
            } else {
                active.clone()
            };
            let combo = egui::ComboBox::from_id_salt(SWITCH_SALT)
                .selected_text(egui::RichText::new(selected).monospace())
                .show_ui(ui, |ui| file_menu_ui(ui, &self.files, path, &active));
            if changed {
                combo.response.on_hover_text(texts.config_file_changed);
            }
            ui.add_space(4.0);
            if icons::icon_button(ui, Icon::Reload)
                .on_hover_text(texts.menu_reload)
                .clicked()
            {
                crate::gui::log::action(texts.menu_reload);
                crate::gui::request_reload();
            }
        }
        ui.add_space(gap);
        let right = if editing {
            // Reading order Save, Revert (§2.4).
            let save = icons::button(ui, Icon::Floppy, texts.config_save);
            if save.clicked() {
                *action = Some(PendingAction::Save);
            }
            let revert = icons::button(ui, Icon::Revert, texts.config_revert);
            if revert.clicked() {
                *action = Some(PendingAction::Revert);
            }
            revert.rect.right()
        } else {
            let edit = icons::button(ui, Icon::Pencil, texts.config_edit);
            if edit.clicked() {
                *action = Some(PendingAction::Edit);
            }
            edit.rect.right()
        };
        // The row minus the two things that vary: the path, and the space it
        // did not use. What is left is the same next frame, whatever the path
        // says.
        (right - left - path_width - gap).max(0.0)
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
            crate::gui::log::action(&i18n::action_switch_file(&entry.name));
            crate::gui::choose_config_path(folder.join(&entry.name));
            crate::gui::request_reload();
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
            crate::gui::log::action(texts.config_open_folder);
            if !crate::gui::win32::open_folder(folder) {
                crate::notify::error(&i18n::open_folder_failed(&folder.display().to_string()));
            }
        }
        ui.add_space(NOTE_GAP);
    });
    ui.add_space(f32::from(CELL_PAD));
}

/// Hands the file to whatever the user associated with `.toml`. Moved here
/// from the tray when the menu item became "Settings" (owner decision
/// 2026-07-21).
///
/// A failure is reported rather than swallowed: the button doing nothing at
/// all is exactly the bug ADR 0038 came from.
fn open_in_default_editor(path: &Path) {
    crate::gui::log::action(&i18n::action_open_editor(&path.display().to_string()));
    if !crate::gui::win32::open_in_default_editor(path) {
        crate::notify::error(&i18n::open_editor_failed(&path.display().to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One unit of width per character, so a budget reads as "this many
    /// characters fit". The real measurement is a font galley; what is being
    /// tested here is which characters get to stay.
    fn per_char(text: &str) -> f32 {
        text.chars().count() as f32
    }

    fn elide(path: &str, budget: f32) -> String {
        elide_path(path, budget, per_char)
    }

    #[test]
    fn a_path_that_fits_is_left_alone() {
        assert_eq!(elide(r"C:\winremap", 40.0), r"C:\winremap");
        // Exactly the budget is a fit, not a near miss.
        assert_eq!(elide(r"C:\winremap", 11.0), r"C:\winremap");
    }

    /// The shape the owner's own folder has: a deep path whose two ends are
    /// the only parts worth reading (report 2026-08-16).
    #[test]
    fn a_deep_path_keeps_the_drive_and_the_deepest_folders() {
        let path = r"C:\Users\suganuma\AppData\Local\Temp\claude\scratchpad\appdata\winremap";
        assert_eq!(elide(path, 30.0), r"C:\…\appdata\winremap");
        // A wider window keeps more of the tail, not more of the middle.
        assert_eq!(
            elide(path, 40.0),
            r"C:\…\claude\scratchpad\appdata\winremap"
        );
    }

    /// Nothing structural fits, so the drive goes too and the folder's name
    /// is eaten from the front — the end of it is what tells two sibling
    /// folders apart.
    #[test]
    fn a_hopeless_budget_keeps_the_end_of_the_folder_name() {
        let path = r"C:\Users\suganuma\AppData\winremap";
        assert_eq!(elide(path, 6.0), "…remap");
        assert_eq!(elide(path, 0.0), "…");
    }

    /// A folder name with no separator at all still shortens rather than
    /// overflowing.
    #[test]
    fn a_path_of_one_piece_is_still_shortened() {
        assert_eq!(elide("averylongfoldername", 6.0), "…rname");
    }

    /// Multi-byte characters are the ordinary case in a Japanese user's
    /// folder, and slicing one in half would panic.
    #[test]
    fn a_japanese_folder_name_is_cut_on_a_character_boundary() {
        let elided = elide(r"C:\ユーザー\設定フォルダー", 5.0);
        assert!(
            elided.chars().count() as f32 <= 5.0,
            "{elided} is wider than the budget"
        );
        assert!(elided.starts_with('…'), "{elided} should say it was cut");
    }
}
