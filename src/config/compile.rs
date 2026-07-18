//! Validation and compilation of raw config sections into the runtime
//! keymap structures.

use std::collections::HashMap;

use toml::Spanned;

use super::Issue;
use super::raw::RawTarget;
use crate::keymap::{
    AppFilter, InputPattern, KeyCombo, MAX_MACRO_LEN, Output, is_modifier_vk, parse_input_pattern,
    parse_key_combo,
};

/// Accumulates one keymap's rules, pushing positioned issues as it goes.
pub(super) struct KeymapCompiler<'a> {
    name: &'a str,
    source: &'a str,
    issues: &'a mut Vec<Issue>,
    pub(super) exact: HashMap<KeyCombo, Output>,
    pub(super) bare: HashMap<u16, u16>,
    pub(super) seqs: HashMap<KeyCombo, HashMap<KeyCombo, Output>>,
}

impl<'a> KeymapCompiler<'a> {
    pub(super) fn new(name: &'a str, source: &'a str, issues: &'a mut Vec<Issue>) -> Self {
        Self {
            name,
            source,
            issues,
            exact: HashMap::new(),
            bare: HashMap::new(),
            seqs: HashMap::new(),
        }
    }

    fn issue(&mut self, offset: usize, message: &str) {
        self.issues
            .push(issue_at(self.source, offset, self.name, message));
    }

    pub(super) fn add_rule(&mut self, lhs: &Spanned<String>, rhs: &Spanned<RawTarget>) {
        let pattern = match parse_input_pattern(lhs.get_ref()) {
            Ok(p) => p,
            Err(e) => {
                self.issue(lhs.span().start, &e.to_string());
                return;
            }
        };
        let Some(output) = self.compile_target(rhs) else {
            return;
        };

        match pattern {
            InputPattern::Single(input) => {
                if is_modifier_vk(input.vk) {
                    self.issue(
                        lhs.span().start,
                        &format!("modifier key `{}` cannot be a remap input", lhs.get_ref()),
                    );
                    return;
                }
                if input.mods.is_empty() {
                    self.add_bare_rule(input.vk, output, lhs, rhs);
                } else {
                    // A combo cannot be both a plain rule and a sequence
                    // prefix — the hook could not tell whether to emit or
                    // wait for a second stroke.
                    if self.seqs.contains_key(&input) {
                        self.issue(
                            lhs.span().start,
                            &format!("`{}` is already used as a sequence prefix", lhs.get_ref()),
                        );
                        return;
                    }
                    if self.exact.insert(input, output).is_some() {
                        self.duplicate(lhs);
                    }
                }
            }
            InputPattern::Sequence(first, second) => {
                if is_modifier_vk(first.vk) || is_modifier_vk(second.vk) {
                    self.issue(
                        lhs.span().start,
                        &format!(
                            "modifier keys cannot appear as strokes in `{}`",
                            lhs.get_ref()
                        ),
                    );
                    return;
                }
                if self.exact.contains_key(&first) {
                    self.issue(
                        lhs.span().start,
                        &format!(
                            "sequence prefix in `{}` is already a plain rule",
                            lhs.get_ref()
                        ),
                    );
                    return;
                }
                if self
                    .seqs
                    .entry(first)
                    .or_default()
                    .insert(second, output)
                    .is_some()
                {
                    self.duplicate(lhs);
                }
            }
        }
    }

    fn add_bare_rule(
        &mut self,
        input_vk: u16,
        output: Output,
        lhs: &Spanned<String>,
        rhs: &Spanned<RawTarget>,
    ) {
        // Bare rules substitute the key while leaving physical modifiers
        // untouched, so the only meaningful output is a single plain key.
        let target_vk = match output {
            Output::Chord(target) if target.mods.is_empty() => target.vk,
            Output::Chord(_) => {
                self.issue(
                    rhs.span().start,
                    "a bare-key rule's target may not have modifiers",
                );
                return;
            }
            Output::Seq(_) => {
                self.issue(
                    rhs.span().start,
                    "a bare-key rule's target may not be a macro",
                );
                return;
            }
        };
        if self.bare.insert(input_vk, target_vk).is_some() {
            self.duplicate(lhs);
        }
    }

    fn compile_target(&mut self, rhs: &Spanned<RawTarget>) -> Option<Output> {
        let offset = rhs.span().start;
        match rhs.get_ref() {
            RawTarget::Single(spec) => match parse_key_combo(spec) {
                Ok(combo) => Some(Output::Chord(combo)),
                Err(e) => {
                    self.issue(offset, &e.to_string());
                    None
                }
            },
            RawTarget::Sequence(specs) => {
                if specs.is_empty() {
                    self.issue(offset, "macro target must not be empty");
                    return None;
                }
                if specs.len() > MAX_MACRO_LEN {
                    self.issue(
                        offset,
                        &format!("macro target exceeds {MAX_MACRO_LEN} strokes"),
                    );
                    return None;
                }
                let mut seq = Vec::with_capacity(specs.len());
                for spec in specs {
                    match parse_key_combo(spec) {
                        Ok(combo) => seq.push(combo),
                        Err(e) => {
                            self.issue(offset, &format!("in macro element `{spec}`: {e}"));
                            return None;
                        }
                    }
                }
                Some(Output::Seq(seq))
            }
        }
    }

    // TOML rejects duplicate literal keys, but distinct spellings ("C-h" vs
    // "c-H") normalize to the same combo; catch those here.
    fn duplicate(&mut self, lhs: &Spanned<String>) {
        self.issue(
            lhs.span().start,
            &format!(
                "`{}` duplicates an earlier rule for the same key",
                lhs.get_ref()
            ),
        );
    }
}

pub(super) fn compile_app_filter(
    name: &str,
    application: &Spanned<Vec<String>>,
    exclude: Option<&Spanned<Vec<String>>>,
    source: &str,
    issues: &mut Vec<Issue>,
) -> AppFilter {
    let entries = application.get_ref();
    let offset = application.span().start;
    if entries.is_empty() {
        issues.push(issue_at(
            source,
            offset,
            name,
            "`application` must not be empty; use [\"*\"] for a global keymap",
        ));
        return AppFilter::Names(Vec::new());
    }

    let is_wildcard = entries.iter().any(|e| e == "*");
    if is_wildcard && entries.len() > 1 {
        // Mixing "*" with concrete names would make the section's scope
        // ambiguous, so it is rejected rather than silently widened.
        issues.push(issue_at(
            source,
            offset,
            name,
            "wildcard \"*\" cannot be combined with specific application names",
        ));
    }
    if let Some(empty) = entries.iter().find(|e| e.trim().is_empty()) {
        issues.push(issue_at(
            source,
            offset,
            name,
            &format!("empty application name `{empty}`"),
        ));
    }

    match exclude {
        Some(excluded) if !is_wildcard => {
            // Excluding from an explicit list is contradictory — just don't
            // list the app. Only global keymaps take exclusions (ADR 0011).
            issues.push(issue_at(
                source,
                excluded.span().start,
                name,
                "`exclude` requires application = [\"*\"]",
            ));
            AppFilter::Names(entries.clone())
        }
        Some(excluded) => {
            let list = excluded.get_ref();
            if let Some(empty) = list.iter().find(|e| e.trim().is_empty()) {
                issues.push(issue_at(
                    source,
                    excluded.span().start,
                    name,
                    &format!("empty application name `{empty}` in `exclude`"),
                ));
            }
            AppFilter::All {
                exclude: list.clone(),
            }
        }
        None if is_wildcard => AppFilter::All {
            exclude: Vec::new(),
        },
        None => AppFilter::Names(entries.clone()),
    }
}

/// Positioned issue for top-level (non-keymap) fields.
pub(super) fn issue_at_offset(source: &str, offset: usize, message: &str) -> Issue {
    let (line, column) = line_col(source, offset);
    Issue {
        line,
        column,
        message: message.to_string(),
    }
}

fn issue_at(source: &str, offset: usize, keymap_name: &str, message: &str) -> Issue {
    let (line, column) = line_col(source, offset);
    Issue {
        line,
        column,
        message: format!("in `{keymap_name}`: {message}"),
    }
}

/// 1-based line/column for a byte offset. Columns count bytes, which is
/// accurate enough since key notation is ASCII.
fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
    let column = prefix.rsplit('\n').next().unwrap_or("").len() + 1;
    (line, column)
}
