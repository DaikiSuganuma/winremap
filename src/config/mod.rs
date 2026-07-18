//! TOML configuration loading, validation, and compilation into the
//! read-only [`RemapTable`] the hook resolves against.
//!
//! Validation reports *all* semantic issues at once with line/column info
//! (config-spec §4) so users can fix a config in one pass.
//!
//! Submodules: `raw` (serde-facing structures), `compile` (validation and
//! compilation into runtime structures).

mod compile;
mod raw;
#[cfg(test)]
mod tests;

use std::fmt;
use std::path::Path;

use crate::keymap::{Keymap, RemapTable};
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
pub fn load(path: &Path) -> Result<RemapTable, ConfigError> {
    let source = std::fs::read_to_string(path)?;
    parse_str(&source)
}

/// Parses and validates TOML source into a [`RemapTable`].
pub fn parse_str(source: &str) -> Result<RemapTable, ConfigError> {
    let raw: RawConfig = toml::from_str(source)?;

    let mut issues = Vec::new();
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

        let mut compiler = KeymapCompiler::new(&name, source, &mut issues);
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

    if issues.is_empty() {
        Ok(RemapTable { keymaps })
    } else {
        Err(ConfigError::Invalid(issues))
    }
}
