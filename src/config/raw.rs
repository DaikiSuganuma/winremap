//! Serde-facing raw structures, deserialized 1:1 from the TOML file before
//! validation. Spanned wrappers keep the byte offsets needed to turn
//! semantic errors into line/column positions after deserialization.

use std::collections::BTreeMap;

use serde::Deserialize;
use toml::Spanned;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawConfig {
    /// `[macro]` — pacing for macro outputs (ADR 0039).
    #[serde(default, rename = "macro")]
    pub(super) macro_section: Option<RawMacro>,
    /// The v0.1 spelling of `[macro] delay_ms`, still accepted so configs
    /// written for 0.1 keep working (ADR 0039).
    #[serde(default)]
    pub(super) macro_delay_ms: Option<Spanned<u32>>,
    /// IME status indicator section (ADR 0020, config-spec §6).
    #[serde(default)]
    pub(super) ime_indicator: Option<RawImeIndicator>,
    #[serde(default)]
    pub(super) keymap: Vec<RawKeymap>,
}

/// `[macro]` — how macro outputs are paced (ADR 0018/0019/0039).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawMacro {
    #[serde(default)]
    pub(super) delay_ms: Option<Spanned<u32>>,
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
    /// Show the target app's exe name under the glyph (ADR 0024).
    #[serde(default)]
    pub(super) show_app_name: Option<bool>,
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
