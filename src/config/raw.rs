//! Serde-facing raw structures, deserialized 1:1 from the TOML file before
//! validation. Spanned wrappers keep the byte offsets needed to turn
//! semantic errors into line/column positions after deserialization.

use std::collections::BTreeMap;

use serde::Deserialize;
use toml::Spanned;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawConfig {
    /// Top-level pacing for macro outputs, in ms (ADR 0019).
    #[serde(default)]
    pub(super) macro_delay_ms: Option<Spanned<u32>>,
    /// IME status indicator section (ADR 0020, config-spec §6).
    #[serde(default)]
    pub(super) ime_indicator: Option<RawImeIndicator>,
    #[serde(default)]
    pub(super) keymap: Vec<RawKeymap>,
}

/// `[ime_indicator]` — every field optional so users can set just `enabled`.
/// Integers are u32 here even for `opacity` so range violations surface as
/// our line-numbered issues instead of serde's type errors.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawImeIndicator {
    #[serde(default)]
    pub(super) enabled: Option<bool>,
    #[serde(default)]
    pub(super) duration_ms: Option<Spanned<u32>>,
    #[serde(default)]
    pub(super) size: Option<Spanned<u32>>,
    #[serde(default)]
    pub(super) opacity: Option<Spanned<u32>>,
    /// Extra toggle chords in key notation, e.g. `["C-Space"]` (ADR 0021).
    #[serde(default)]
    pub(super) trigger_keys: Option<Vec<Spanned<String>>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawKeymap {
    #[serde(default)]
    pub(super) name: Option<String>,
    pub(super) application: Spanned<Vec<String>>,
    #[serde(default)]
    pub(super) exclude: Option<Spanned<Vec<String>>>,
    #[serde(default)]
    pub(super) remap: BTreeMap<Spanned<String>, Spanned<RawTarget>>,
}

/// RHS of a rule: a single chord string, or an array of chords (macro).
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum RawTarget {
    Single(String),
    Sequence(Vec<String>),
}
