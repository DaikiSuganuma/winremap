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

use crate::ime_indicator_settings::{
    IndicatorSettings, MAX_INDICATOR_DURATION_MS, MAX_INDICATOR_SIZE, MIN_INDICATOR_DURATION_MS,
    MIN_INDICATOR_SIZE,
};
use crate::keymap::{Keymap, MAX_MACRO_DELAY_MS, RemapTable, is_modifier_vk, parse_key_combo};
use compile::{KeymapCompiler, compile_app_filter, issue_at_offset};
use raw::{RawConfig, RawImeIndicator};

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

    let macro_delay_ms = match &raw.macro_delay_ms {
        Some(delay) if *delay.get_ref() > MAX_MACRO_DELAY_MS => {
            issues.push(issue_at_offset(
                source,
                delay.span().start,
                &format!(
                    "`macro_delay_ms` must be 0-{MAX_MACRO_DELAY_MS} (got {})",
                    delay.get_ref()
                ),
            ));
            0
        }
        Some(delay) => *delay.get_ref(),
        None => 0,
    };

    let ime_indicator = compile_ime_indicator(raw.ime_indicator, source, &mut issues);

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
        Ok(RemapTable {
            keymaps,
            macro_delay_ms,
            ime_indicator,
        })
    } else {
        Err(ConfigError::Invalid(issues))
    }
}

/// Validates the `[ime_indicator]` section against the ranges in
/// config-spec §6, falling back to the field's default on violation so all
/// issues are still collected in one pass.
fn compile_ime_indicator(
    raw: Option<RawImeIndicator>,
    source: &str,
    issues: &mut Vec<Issue>,
) -> IndicatorSettings {
    let defaults = IndicatorSettings::default();
    let Some(raw) = raw else {
        return defaults;
    };
    let mut ranged =
        |field: Option<toml::Spanned<u32>>, name: &str, min: u32, max: u32, def| match field {
            Some(value) if !(min..=max).contains(value.get_ref()) => {
                issues.push(issue_at_offset(
                    source,
                    value.span().start,
                    &format!(
                        "`ime_indicator.{name}` must be {min}-{max} (got {})",
                        value.get_ref()
                    ),
                ));
                def
            }
            Some(value) => *value.get_ref(),
            None => def,
        };
    let duration_ms = ranged(
        raw.duration_ms,
        "duration_ms",
        MIN_INDICATOR_DURATION_MS,
        MAX_INDICATOR_DURATION_MS,
        defaults.duration_ms,
    );
    let size = ranged(
        raw.size,
        "size",
        MIN_INDICATOR_SIZE,
        MAX_INDICATOR_SIZE,
        defaults.size,
    );
    let opacity = ranged(raw.opacity, "opacity", 0, 255, defaults.opacity.into());
    let mut trigger_keys = Vec::new();
    for item in raw.trigger_keys.into_iter().flatten() {
        match parse_key_combo(item.get_ref()) {
            // Modifier keys never reach the indicator's key check (the hook
            // consumes them for chord state first), so reject them here
            // instead of silently never firing.
            Ok(combo) if is_modifier_vk(combo.vk) => issues.push(issue_at_offset(
                source,
                item.span().start,
                &format!(
                    "`ime_indicator.trigger_keys`: modifier key `{}` cannot be a trigger",
                    item.get_ref()
                ),
            )),
            Ok(combo) => trigger_keys.push(combo),
            Err(e) => issues.push(issue_at_offset(
                source,
                item.span().start,
                &format!("`ime_indicator.trigger_keys`: {e}"),
            )),
        }
    }
    IndicatorSettings {
        enabled: raw.enabled.unwrap_or(defaults.enabled),
        duration_ms,
        size,
        // Cast is lossless: the 0-255 range was just enforced above.
        opacity: opacity as u8,
        trigger_keys,
    }
}
