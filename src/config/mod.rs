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
mod raw;
#[cfg(test)]
mod tests;

use std::fmt;
use std::path::Path;

use crate::ime_indicator_settings::{
    IndicatorSettings, MAX_INDICATOR_DURATION_MS, MAX_INDICATOR_SIZE, MIN_INDICATOR_DURATION_MS,
    MIN_INDICATOR_SIZE,
};
use crate::keymap::{
    KeyCombo, Keymap, MAX_MACRO_DELAY_MS, RemapTable, is_modifier_vk, parse_key_combo,
};
use crate::recorder::RecordKeys;
use compile::{KeymapCompiler, compile_app_filter, issue_at_offset};
use raw::{RawConfig, RawImeIndicator, RawMacro};

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

    let macro_delay_ms = compile_macro_delay(&raw, source, &mut issues);
    let macro_record = compile_macro_record(raw.macro_section.as_ref(), source, &mut issues);

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

/// Record keys plus where each was written, so a collision found after the
/// keymaps are compiled can still point at the `[macro]` line.
struct CompiledRecordKeys {
    keys: RecordKeys,
    /// `(label, key, byte offset)` for each configured key. `record_stop`
    /// shares `record_start`'s offset when it was omitted.
    positions: Vec<(&'static str, KeyCombo, usize)>,
}

/// Validates the recording keys in `[macro]` (ADR 0043, design doc §4).
/// Returns `None` when the feature is off — writing none of the three keys
/// is the normal, supported state.
fn compile_macro_record(
    raw: Option<&RawMacro>,
    source: &str,
    issues: &mut Vec<Issue>,
) -> Option<CompiledRecordKeys> {
    let raw = raw?;
    if raw.record_start.is_none() && raw.record_stop.is_none() && raw.record_play.is_none() {
        return None;
    }

    let mut parse_field = |field: Option<&toml::Spanned<String>>, name: &str| match field {
        None => None,
        Some(spanned) => match parse_key_combo(spanned.get_ref()) {
            // A modifier key never reaches this check in the hook — the
            // callback consumes modifiers for chord state before any lookup —
            // so reject it here instead of leaving a key that never fires.
            Ok(combo) if is_modifier_vk(combo.vk) => {
                issues.push(issue_at_offset(
                    source,
                    spanned.span().start,
                    &format!(
                        "`macro.{name}`: modifier key `{}` cannot be a recording key",
                        spanned.get_ref()
                    ),
                ));
                None
            }
            Ok(combo) => Some((combo, spanned.span().start)),
            Err(e) => {
                issues.push(issue_at_offset(
                    source,
                    spanned.span().start,
                    &format!("`macro.{name}`: {e}"),
                ));
                None
            }
        },
    };

    let start = parse_field(raw.record_start.as_ref(), "record_start");
    let stop = parse_field(raw.record_stop.as_ref(), "record_stop");
    let play = parse_field(raw.record_play.as_ref(), "record_play");

    // Recording needs a way in and a way to use the result; either alone is
    // a half-configured feature, which is more likely a mistake than intent.
    let (Some((start, start_offset)), Some((play, play_offset))) = (start, play) else {
        // Only complain about the missing half when the present half parsed;
        // a parse error above already said what is wrong with this section.
        if issues.is_empty() {
            let offset = raw
                .record_start
                .as_ref()
                .or(raw.record_play.as_ref())
                .or(raw.record_stop.as_ref())
                .map_or(0, |spanned| spanned.span().start);
            issues.push(issue_at_offset(
                source,
                offset,
                "`macro.record_start` and `macro.record_play` must both be set to enable macro recording",
            ));
        }
        return None;
    };

    // Omitted stop means the start key toggles, which is the documented
    // default shape (`S-F10` starts, `S-F10` ends).
    let (stop, stop_offset) = stop.unwrap_or((start, start_offset));

    if play == start || play == stop {
        issues.push(issue_at_offset(
            source,
            play_offset,
            &format!(
                "`macro.record_play` (`{play}`) must differ from the recording keys; one key cannot both end a recording and replay it"
            ),
        ));
        return None;
    }

    Some(CompiledRecordKeys {
        keys: RecordKeys { start, stop, play },
        positions: vec![
            ("record_start", start, start_offset),
            ("record_stop", stop, stop_offset),
            ("record_play", play, play_offset),
        ],
    })
}

/// Rejects a recording key that a keymap also remaps.
///
/// Record keys are intercepted before any keymap lookup and apply to every
/// application, so such a rule can never fire — and nothing in the config
/// file shows that. Unlike keymap-vs-keymap overlaps, which express a real
/// priority (app-specific beats global) and are only surfaced in the
/// settings window, this is silent dead configuration (design doc §4).
fn check_record_key_collisions(
    record: &CompiledRecordKeys,
    keymaps: &[Keymap],
    source: &str,
    issues: &mut Vec<Issue>,
) {
    for (index, &(name, key, offset)) in record.positions.iter().enumerate() {
        // A toggle key is listed twice (start and stop); report it once.
        if record.positions[..index]
            .iter()
            .any(|&(_, earlier, _)| earlier == key)
        {
            continue;
        }
        for keymap in keymaps {
            // Bare rules match on the VK alone, so `F10 = "Home"` shadows
            // every chord on F10, `S-F10` included.
            let shadowed = keymap.exact.contains_key(&key)
                || keymap.seqs.contains_key(&key)
                || keymap.bare.contains_key(&key.vk);
            if shadowed {
                issues.push(issue_at_offset(
                    source,
                    offset,
                    &format!(
                        "`macro.{name}` (`{key}`) is also remapped in `{}`; recording keys are always taken first, so that rule would never fire",
                        keymap.name
                    ),
                ));
            }
        }
    }
}

/// Validates the `[ime_indicator]` section against the ranges in
/// config-spec §6, falling back to the field's default on violation so all
/// issues are still collected in one pass.
/// `[macro] delay_ms`, falling back to the v0.1 top-level `macro_delay_ms`
/// (ADR 0039). Setting both is an error rather than a silent precedence: the
/// user would otherwise never learn which one is in effect.
fn compile_macro_delay(raw: &RawConfig, source: &str, issues: &mut Vec<Issue>) -> u32 {
    let new = raw.macro_section.as_ref().and_then(|m| m.delay_ms.as_ref());
    let old = raw.macro_delay_ms.as_ref();
    if let (Some(new), Some(old)) = (new, old) {
        issues.push(issue_at_offset(
            source,
            new.span().start.max(old.span().start),
            "both `[macro] delay_ms` and the deprecated top-level `macro_delay_ms` are set; keep only `[macro] delay_ms`",
        ));
    }
    let (spanned, key) = match (new, old) {
        (Some(new), _) => (new, "[macro] delay_ms"),
        (None, Some(old)) => (old, "macro_delay_ms"),
        (None, None) => return 0,
    };
    let value = *spanned.get_ref();
    if value > MAX_MACRO_DELAY_MS {
        issues.push(issue_at_offset(
            source,
            spanned.span().start,
            &format!("`{key}` must be 0-{MAX_MACRO_DELAY_MS} (got {value})"),
        ));
        return 0;
    }
    value
}

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
        show_app_name: raw.show_app_name.unwrap_or(defaults.show_app_name),
    }
}
