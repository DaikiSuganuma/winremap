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
    #[serde(default)]
    pub(super) keymap: Vec<RawKeymap>,
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
