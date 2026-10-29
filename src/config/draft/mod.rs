//! The editable draft of a config file (ADR 0049, milestone B2).
//!
//! Two models exist side by side: the *view* is the live, compiled
//! [`crate::keymap::RemapTable`] the hook resolves against, and the *edit* is
//! this draft — built from the file itself, holding the exact strings the
//! user wrote (`"S-C-h"` stays `"S-C-h"`, `"Bak"` mid-typing is fine). The
//! draft never touches the live table; saving applies the draft's *changes*
//! to a freshly read `toml_edit::DocumentMut` so everything the user did not
//! edit — comments, blank lines, ordering, spellings — survives verbatim
//! (ADR 0036).
//!
//! Rule keys the draft writes are quoted, matching how the rules around them
//! are written; `toml_edit`'s own encoder would leave `C-y` bare, which is the
//! same TOML but reads as a different kind of line (owner feedback
//! 2026-07-26).

//!
//! Split by direction (v1.1 plan A4): this file is the draft itself and the
//! read side (`parse`); `apply` is the save side, which diffs two drafts
//! into a `toml_edit` document; `rebuild` is the branch of that for a
//! keymap list whose shape changed; `toml` is the value-to-text and
//! text-to-value plumbing both sides share.

mod apply;
mod rebuild;
mod toml;

pub use apply::{apply, write_atomic};

use std::path::Path;
use std::time::SystemTime;

use toml_edit::{DocumentMut, Item, Table};

use toml::{output_text, scalar_text, string_list};

/// A whole config file as editable strings, in file order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfigDraft {
    pub keymaps: Vec<KeymapDraft>,
    /// `[macro] delay_ms` as written; empty = the key is absent.
    pub macro_delay: String,
    pub macro_record: MacroRecordDraft,
    pub ime: ImeIndicatorDraft,
    /// The file spells the delay `macro_delay_ms` (v0.1, ADR 0039). Saving
    /// keeps whichever spelling the file already uses.
    pub uses_legacy_delay_key: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeymapDraft {
    /// Index of the `[[keymap]]` this section came from, `None` for one the
    /// user added. The diff keys off it, so reordering and renaming still
    /// find the original table with all its formatting.
    pub origin: Option<usize>,
    pub name: String,
    pub application: Vec<String>,
    pub exclude: Vec<String>,
    pub rules: Vec<RuleDraft>,
}

/// One remap rule, both sides as the raw key-notation strings. A macro
/// output is comma-separated, matching what the user would write in the
/// file's array form.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleDraft {
    /// Index into the original keymap's rule list; see [`KeymapDraft::origin`].
    pub origin: Option<usize>,
    pub input: String,
    pub output: String,
}

/// `[macro] record_*` keys as written; empty = absent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MacroRecordDraft {
    pub start: String,
    pub stop: String,
    pub play: String,
}

/// `[ime_indicator]` as written. Numbers stay strings so a half-typed value
/// survives a frame; `None` for a bool means the key is absent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImeIndicatorDraft {
    pub enabled: Option<bool>,
    pub duration_ms: String,
    pub size: String,
    pub opacity: String,
    pub show_app_name: Option<bool>,
    /// Comma-separated, like a macro output.
    pub trigger_keys: String,
    /// Independent of `enabled` (ADR 0067), so it is edited whether or not
    /// the panel is switched on.
    pub change_cursor_color: Option<bool>,
    /// `#rrggbb` as written. Kept as text for the same reason the numbers
    /// are: a half-typed colour has to survive the frame it is typed in.
    pub cursor_color: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    // toml_edit's error renders line/column plus a source snippet.
    #[error("TOML syntax error:\n{0}")]
    Toml(#[from] toml_edit::TomlError),
}

/// What the file looked like when it was read: enough to notice it changed
/// behind our back (external-change detection, design doc §6.2), nothing
/// that requires reading its contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub modified: Option<SystemTime>,
    pub len: u64,
}

pub fn stamp(path: &Path) -> std::io::Result<FileStamp> {
    let meta = std::fs::metadata(path)?;
    Ok(FileStamp {
        modified: meta.modified().ok(),
        len: meta.len(),
    })
}

/// Reads a file into a draft, with the stamp for external-change detection.
/// The only hard failure is a file that cannot be read or is not TOML at
/// all; questionable *values* stay editable and are validation's problem.
pub fn read(path: &Path) -> Result<(ConfigDraft, FileStamp), DraftError> {
    let stamp = stamp(path)?;
    let source = std::fs::read_to_string(path)?;
    Ok((parse(&source)?, stamp))
}

pub fn parse(source: &str) -> Result<ConfigDraft, toml_edit::TomlError> {
    let doc: DocumentMut = source.parse()?;
    let mut draft = ConfigDraft::default();

    if let Some(item) = doc.get("macro_delay_ms") {
        draft.uses_legacy_delay_key = true;
        draft.macro_delay = scalar_text(item);
    }
    if let Some(section) = doc.get("macro").and_then(Item::as_table) {
        if let Some(item) = section.get("delay_ms") {
            draft.macro_delay = scalar_text(item);
        }
        draft.macro_record = MacroRecordDraft {
            start: section
                .get("record_start")
                .map(scalar_text)
                .unwrap_or_default(),
            stop: section
                .get("record_stop")
                .map(scalar_text)
                .unwrap_or_default(),
            play: section
                .get("record_play")
                .map(scalar_text)
                .unwrap_or_default(),
        };
    }
    if let Some(section) = doc.get("ime_indicator").and_then(Item::as_table) {
        draft.ime = ImeIndicatorDraft {
            enabled: section.get("enabled").and_then(Item::as_bool),
            duration_ms: section
                .get("duration_ms")
                .map(scalar_text)
                .unwrap_or_default(),
            size: section.get("size").map(scalar_text).unwrap_or_default(),
            opacity: section.get("opacity").map(scalar_text).unwrap_or_default(),
            show_app_name: section.get("show_app_name").and_then(Item::as_bool),
            trigger_keys: section
                .get("trigger_keys")
                .map(output_text)
                .unwrap_or_default(),
            change_cursor_color: section.get("change_cursor_color").and_then(Item::as_bool),
            cursor_color: section
                .get("cursor_color")
                .map(scalar_text)
                .unwrap_or_default(),
        };
    }
    if let Some(keymaps) = doc.get("keymap").and_then(Item::as_array_of_tables) {
        for (index, table) in keymaps.iter().enumerate() {
            draft.keymaps.push(keymap_draft(index, table));
        }
    }
    Ok(draft)
}

fn keymap_draft(index: usize, table: &Table) -> KeymapDraft {
    let rules = table
        .get("remap")
        .and_then(Item::as_table)
        .map(|remap| {
            remap
                .iter()
                .enumerate()
                .map(|(i, (input, item))| RuleDraft {
                    origin: Some(i),
                    input: input.to_owned(),
                    output: output_text(item),
                })
                .collect()
        })
        .unwrap_or_default();
    KeymapDraft {
        origin: Some(index),
        name: table
            .get("name")
            .and_then(Item::as_str)
            .unwrap_or_default()
            .to_owned(),
        application: string_list(table.get("application")),
        exclude: string_list(table.get("exclude")),
        rules,
    }
}
