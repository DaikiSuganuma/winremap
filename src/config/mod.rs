//! TOML configuration loading, validation, and compilation into the
//! read-only [`RemapTable`] the hook resolves against.
//!
//! Validation reports *all* semantic issues at once with line/column info
//! (config-spec §4) so users can fix a config in one pass.
//!
//! Submodules: `raw` (serde-facing structures), `compile` (validation and
//! compilation into runtime structures).

pub mod comments;
mod compile;
pub mod draft;
pub mod last_used;
mod raw;
#[cfg(test)]
mod tests;

use std::fmt;
use std::io::Write;
use std::path::Path;

use crate::keymap::{Keymap, Layout, RemapTable};
use compile::ime::compile_ime_indicator;
use compile::macros::{check_record_key_collisions, compile_macro_delay, compile_macro_record};
use compile::{KeymapCompiler, compile_app_filter};
use raw::RawConfig;

/// One semantic problem in the config, positioned at its source line.
#[derive(Debug, PartialEq, Eq)]
pub struct Issue {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    // toml's error already renders line/column plus a source snippet.
    #[error("TOML syntax error:\n{0}")]
    Toml(#[from] toml::de::Error),
    #[error("{}", format_issues(.0))]
    Invalid(Vec<Issue>),
}

fn format_issues(issues: &[Issue]) -> String {
    let mut out = String::from("invalid configuration:");
    for issue in issues {
        out.push_str("\n  ");
        out.push_str(&issue.to_string());
    }
    out
}

/// Reads and compiles a config file.
///
/// `layout` is what the attached keyboard prints on its symbol keys; the
/// binary reads it from Windows just before calling this, so a keyboard
/// swapped since the last load is picked up by a reload (ADR 0063).
pub fn load(path: &Path, layout: &Layout) -> Result<RemapTable, ConfigError> {
    let source = std::fs::read_to_string(path)?;
    parse_str(&source, layout)
}

/// The config written on first run when the user has none.
///
/// This is the same file the installer seeds, embedded rather than copied so
/// the two can never drift: an installed and a portable WinRemap must start
/// from the same config, and a test here proves it still parses.
pub const DEFAULT_CONFIG: &str = include_str!("../../examples/minimal.toml");

/// Writes [`DEFAULT_CONFIG`] to `path`, creating parent directories.
///
/// Refuses to overwrite an existing file. A config the user wrote is never
/// something to replace, and two WinRemap processes reaching first-run at
/// once must not have the loser clobber the file the winner just wrote —
/// hence `create_new` rather than a check followed by a write.
pub fn create_default(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create_new(path)?;
    file.write_all(DEFAULT_CONFIG.as_bytes())
}

/// Parses and validates TOML source into a [`RemapTable`].
pub fn parse_str(source: &str, layout: &Layout) -> Result<RemapTable, ConfigError> {
    let raw: RawConfig = toml::from_str(source)?;

    let mut issues = Vec::new();

    let macro_delay_ms = compile_macro_delay(&raw, source, &mut issues);
    let macro_record =
        compile_macro_record(raw.macro_section.as_ref(), source, layout, &mut issues);

    let ime_indicator = compile_ime_indicator(raw.ime_indicator, source, layout, &mut issues);

    let mut keymaps = Vec::new();
    for (index, raw_keymap) in raw.keymap.into_iter().enumerate() {
        // Fall back to a positional name so issues stay attributable even
        // when the section is anonymous.
        let name = raw_keymap
            .name
            .unwrap_or_else(|| format!("keymap #{}", index + 1));
        let apps = compile_app_filter(
            &name,
            &raw_keymap.application,
            raw_keymap.exclude.as_ref(),
            source,
            &mut issues,
        );

        let mut compiler = KeymapCompiler::new(&name, source, layout, &mut issues);
        for (lhs, rhs) in &raw_keymap.remap {
            compiler.add_rule(lhs, rhs);
        }
        keymaps.push(Keymap {
            name: name.clone(),
            apps,
            exact: compiler.exact,
            bare: compiler.bare,
            seqs: compiler.seqs,
        });
    }

    if let Some(record) = macro_record.as_ref() {
        check_record_key_collisions(record, &keymaps, source, &mut issues);
    }

    if issues.is_empty() {
        Ok(RemapTable {
            keymaps,
            macro_delay_ms,
            ime_indicator,
            macro_record: macro_record.map(|record| record.keys),
        })
    } else {
        Err(ConfigError::Invalid(issues))
    }
}
