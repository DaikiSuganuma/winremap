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
use crate::theme::{CELL_PAD, EDGE_PAD, NOTE_GAP, SECTION_GAP, SECTION_TEXT, TITLE_TEXT};
use winremap::config::comments::{ConfigComments, KeymapComments};
use winremap::config::draft;
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
    /// The address bar's view of the config folder; see [`FileList`].
    files: FileList,
}

/// How stale the folder listing and the change marks may get. Short enough
/// that an external save shows up while the window is open, long enough
/// that painting stays free of disk access. The notify watch (ADR 0051)
/// will make this poll the fallback rather than the primary path.
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
}

struct FileEntry {
    name: String,
    changed: bool,
}

impl FileList {
    fn refresh(&mut self, path: &Path) {
        if let Some(checked) = self.checked
            && checked.elapsed() < FILE_POLL_INTERVAL
        {
            return;
        }
        self.checked = Some(Instant::now());
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
        let loaded = super::loaded_stamp();
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

        self.header_ui(ui, &path);
        statusbar_ui(ui);

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
    /// the right end — the edit button (owner decision 2026-07-22).
    fn header_ui(&mut self, ui: &mut egui::Ui, path: &Path) {
        let texts = i18n::t();
        egui::Panel::top("config-header")
            .frame(theme::chrome_frame(ui.visuals()))
            .show(ui, |ui| {
                self.files.refresh(path);
                let active = file_name(path);
                ui.horizontal(|ui| {
                    // The row height is fixed up front so every element —
                    // icon, path, dropdown, buttons — centres on one axis.
                    // Without it the shorter labels sit at the top while the
                    // taller widgets, added later, stretch the row downward.
                    let row_height =
                        ui.text_style_height(&egui::TextStyle::Button) + 2.0 * theme::BUTTON_PADDING;
                    ui.set_height(row_height);
                    icons::show(ui, Icon::Folder, theme::body_icon_size(ui));
                    let folder = path
                        .parent()
                        .map(|folder| folder.display().to_string())
                        .unwrap_or_default();
                    ui.label(egui::RichText::new(folder).monospace().weak());
                    ui.label(egui::RichText::new("›").weak());
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Edit-mode entry point (B2). Disabled until the draft
                        // editing lands; placed now so the layout is final.
                        ui.add_enabled_ui(false, |ui| {
                            let _ = icons::button(ui, Icon::Pencil, texts.config_edit);
                        });
                    });
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
        self.comments = winremap::config::comments::read(path);
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
            // any failed reload: the live table stays.
            super::log::action(&i18n::action_switch_file(&entry.name));
            super::set_config_path(folder.join(&entry.name));
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
    ui.separator();
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
    ui.label(
        egui::RichText::new(keymap_label(keymap))
            .size(TITLE_TEXT)
            .strong(),
    );
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
    ui.label(
        egui::RichText::new(texts.config_general)
            .size(TITLE_TEXT)
            .strong(),
    );

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
fn note(ui: &mut egui::Ui, comment: Option<&str>) {
    if let Some(comment) = comment {
        ui.indent("note", |ui| {
            ui.label(egui::RichText::new(format!("# {comment}")).weak());
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
