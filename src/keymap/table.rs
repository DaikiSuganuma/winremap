//! Compiled keymaps and event resolution against them.

use std::collections::HashMap;

use super::KeyCombo;
use crate::ime_indicator_settings::IndicatorSettings;
use crate::recorder::RecordKeys;

/// What an exact or sequence rule emits (config-spec §3, ADR 0012).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Output {
    /// Single chord; the held modifiers are adjusted to match it.
    Chord(KeyCombo),
    /// Macro: tap each chord once, in order, per key press. Auto-repeat of
    /// the source key is ignored so a held key cannot spray the macro.
    Seq(Vec<KeyCombo>),
}

/// Which processes a keymap applies to.
#[derive(Clone, Debug)]
pub enum AppFilter {
    /// `application = ["*"]`, minus the `exclude` list (ADR 0011).
    All { exclude: Vec<String> },
    /// Exact exe names, matched case-insensitively.
    Names(Vec<String>),
}

impl AppFilter {
    /// Case-insensitive because Windows file names are; compares without
    /// allocating so it is safe to call from the hook callback.
    pub fn matches(&self, exe: &str) -> bool {
        match self {
            AppFilter::All { exclude } => !exclude.iter().any(|n| n.eq_ignore_ascii_case(exe)),
            AppFilter::Names(names) => names.iter().any(|n| n.eq_ignore_ascii_case(exe)),
        }
    }

    pub fn is_global(&self) -> bool {
        matches!(self, AppFilter::All { .. })
    }
}

/// One compiled `[[keymap]]` section.
#[derive(Debug)]
pub struct Keymap {
    pub name: String,
    pub apps: AppFilter,
    /// Rules whose input listed modifiers; matched on the full combo.
    pub exact: HashMap<KeyCombo, Output>,
    /// Bare-key rules (input vk → output vk); matched on the VK alone,
    /// ignoring held modifiers.
    pub bare: HashMap<u16, u16>,
    /// Two-stroke sequences: first stroke → (second stroke → output).
    pub seqs: HashMap<KeyCombo, HashMap<KeyCombo, Output>>,
}

/// First-stroke resolution result for the hook.
#[derive(Debug, PartialEq, Eq)]
pub enum Resolution<'a> {
    /// Suppress the event and emit this output.
    Exact(&'a Output),
    /// Bare-key substitution: replace the key, leave modifiers untouched.
    KeyOnly(u16),
    /// First stroke of a sequence: suppress and wait for the next key.
    Prefix,
}

impl Keymap {
    fn lookup(&self, input: KeyCombo) -> Option<Resolution<'_>> {
        // Plain-vs-prefix conflicts are rejected at config time, so the order
        // of the first two checks is only defensive. Bare rules come last so
        // an app can both swap a key and still special-case chords on it
        // (ADR 0004).
        if let Some(output) = self.exact.get(&input) {
            return Some(Resolution::Exact(output));
        }
        if self.seqs.contains_key(&input) {
            return Some(Resolution::Prefix);
        }
        if let Some(&target_vk) = self.bare.get(&input.vk) {
            return Some(Resolution::KeyOnly(target_vk));
        }
        None
    }
}

/// The read-only structure the hook resolves events against. Built by
/// `config::parse_str` and swapped atomically on reload (ADR 0003).
#[derive(Debug)]
pub struct RemapTable {
    pub keymaps: Vec<Keymap>,
    /// Per-stroke macro pacing from the config's top-level `macro_delay_ms`
    /// (0 = burst, ADR 0018/0019). Carried here so a tray reload can apply
    /// the new value together with the rules.
    pub macro_delay_ms: u32,
    /// `[ime_indicator]` settings, carried for the same reload-together
    /// reason. Consumed by the indicator thread, not by key resolution
    /// (ADR 0020; the feature itself lives in src/ime_indicator/).
    pub ime_indicator: IndicatorSettings,
    /// `[macro]` recording keys, or `None` when the user configured none and
    /// the feature stays off (ADR 0043). Read from the hook callback to
    /// decide whether a key is a record key before any keymap lookup.
    pub macro_record: Option<RecordKeys>,
}

impl RemapTable {
    /// Resolves a key event for the foreground process `exe`.
    ///
    /// Runs inside the low-level hook: must not allocate or block (AGENTS.md
    /// invariant 2). App-specific keymaps win over `*` ones regardless of
    /// definition order; within a class, first match wins (ADR 0004). `None`
    /// means "pass the event through unchanged".
    pub fn resolve(&self, exe: &str, input: KeyCombo) -> Option<Resolution<'_>> {
        // The global pass must also call matches(): "*" keymaps still reject
        // the exes on their exclude list (ADR 0011).
        self.keymaps
            .iter()
            .filter(|k| !k.apps.is_global() && k.apps.matches(exe))
            .find_map(|k| k.lookup(input))
            .or_else(|| {
                self.keymaps
                    .iter()
                    .filter(|k| k.apps.is_global() && k.apps.matches(exe))
                    .find_map(|k| k.lookup(input))
            })
    }

    /// Resolves the second stroke after `first` was recognized as a prefix.
    /// Same priority order as `resolve`; `None` means the sequence is
    /// undefined (the hook swallows the stroke, Emacs-style).
    pub fn resolve_second(&self, exe: &str, first: KeyCombo, second: KeyCombo) -> Option<&Output> {
        self.keymaps
            .iter()
            .filter(|k| !k.apps.is_global() && k.apps.matches(exe))
            .find_map(|k| k.seqs.get(&first).and_then(|m| m.get(&second)))
            .or_else(|| {
                self.keymaps
                    .iter()
                    .filter(|k| k.apps.is_global() && k.apps.matches(exe))
                    .find_map(|k| k.seqs.get(&first).and_then(|m| m.get(&second)))
            })
    }
}
