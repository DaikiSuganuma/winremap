//! TOML configuration loading, validation, and compilation into the
//! read-only [`RemapTable`] the hook resolves against.
//!
//! Validation reports *all* semantic issues at once with line/column info
//! (config-spec §4) so users can fix a config in one pass.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::Path;

use serde::Deserialize;
use toml::Spanned;

use crate::keymap::{
    AppFilter, InputPattern, Keymap, MAX_MACRO_LEN, Output, RemapTable, is_modifier_vk,
    parse_input_pattern, parse_key_combo,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    keymap: Vec<RawKeymap>,
}

// Spanned wrappers keep the byte offsets we need to turn semantic errors
// into line/column positions after deserialization.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKeymap {
    #[serde(default)]
    name: Option<String>,
    application: Spanned<Vec<String>>,
    #[serde(default)]
    exclude: Option<Spanned<Vec<String>>>,
    #[serde(default)]
    remap: BTreeMap<Spanned<String>, Spanned<RawTarget>>,
}

/// RHS of a rule: a single chord string, or an array of chords (macro).
#[derive(Deserialize)]
#[serde(untagged)]
enum RawTarget {
    Single(String),
    Sequence(Vec<String>),
}

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

        let mut compiler = KeymapCompiler {
            name: &name,
            source,
            issues: &mut issues,
            exact: HashMap::new(),
            bare: HashMap::new(),
            seqs: HashMap::new(),
        };
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

/// Accumulates one keymap's rules, pushing positioned issues as it goes.
struct KeymapCompiler<'a> {
    name: &'a str,
    source: &'a str,
    issues: &'a mut Vec<Issue>,
    exact: HashMap<crate::keymap::KeyCombo, Output>,
    bare: HashMap<u16, u16>,
    seqs: HashMap<crate::keymap::KeyCombo, HashMap<crate::keymap::KeyCombo, Output>>,
}

impl KeymapCompiler<'_> {
    fn issue(&mut self, offset: usize, message: &str) {
        self.issues
            .push(issue_at(self.source, offset, self.name, message));
    }

    fn add_rule(&mut self, lhs: &Spanned<String>, rhs: &Spanned<RawTarget>) {
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

fn compile_app_filter(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{KeyCombo, Resolution};

    fn combo(spec: &str) -> KeyCombo {
        parse_key_combo(spec).unwrap()
    }

    fn issues(source: &str) -> Vec<Issue> {
        match parse_str(source) {
            Err(ConfigError::Invalid(issues)) => issues,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn compiles_valid_config() {
        let table = parse_str(
            r#"
[[keymap]]
name = "global"
application = ["*"]

[keymap.remap]
"CapsLock" = "LCtrl"

[[keymap]]
application = ["phpstorm64.exe"]

[keymap.remap]
"C-h" = "Back"
"#,
        )
        .unwrap();
        assert_eq!(table.keymaps.len(), 2);
        assert_eq!(table.keymaps[0].bare.len(), 1);
        assert_eq!(table.keymaps[1].exact.len(), 1);
        // Anonymous sections get a positional fallback name for diagnostics.
        assert_eq!(table.keymaps[1].name, "keymap #2");
    }

    #[test]
    fn compiles_exclude_macro_and_sequence_rules() {
        let table = parse_str(
            r#"
[[keymap]]
name = "emacs"
application = ["*"]
exclude = ["Zed.exe"]

[keymap.remap]
"C-t" = ["C-Right", "C-Left", "C-S-Right"]
"A-x u" = "C-z"
"#,
        )
        .unwrap();
        let keymap = &table.keymaps[0];
        assert!(matches!(
            keymap.exact.get(&combo("C-t")),
            Some(Output::Seq(seq)) if seq.len() == 3
        ));
        assert_eq!(
            table.resolve("zed.exe", combo("C-t")),
            None,
            "excluded app must not match"
        );
        assert_eq!(
            table.resolve("notepad.exe", combo("A-x")),
            Some(Resolution::Prefix)
        );
        assert_eq!(
            table.resolve_second("notepad.exe", combo("A-x"), combo("u")),
            Some(&Output::Chord(combo("C-z")))
        );
    }

    #[test]
    fn reports_syntax_error_from_toml() {
        let err = parse_str("[[keymap]\n").unwrap_err();
        assert!(matches!(err, ConfigError::Toml(_)));
    }

    #[test]
    fn missing_application_is_an_error() {
        let err = parse_str("[[keymap]]\nname = \"x\"\n").unwrap_err();
        // Reported by serde as a missing field, with toml's span rendering.
        assert!(err.to_string().contains("application"), "{err}");
    }

    #[test]
    fn unknown_field_is_an_error() {
        let err = parse_str("[[keymap]]\napplication = [\"*\"]\napplicatoin = [\"x\"]\n");
        assert!(err.is_err());
    }

    #[test]
    fn reports_bad_key_notation_with_line_numbers() {
        let found = issues(
            "\n[[keymap]]\nname = \"broken\"\napplication = [\"*\"]\n\n[keymap.remap]\n\"C-Bcak\" = \"Back\"\n\"C-h\" = \"Nope\"\n",
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].line, 7);
        assert!(found[0].message.contains("broken"));
        assert!(found[0].message.contains("Bcak"));
        assert_eq!(found[1].line, 8);
        assert!(found[1].message.contains("Nope"));
    }

    #[test]
    fn rejects_wildcard_mixed_with_names() {
        let found = issues("[[keymap]]\napplication = [\"*\", \"notepad.exe\"]\n");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("wildcard"));
    }

    #[test]
    fn rejects_empty_application_list() {
        let found = issues("[[keymap]]\napplication = []\n");
        assert!(found[0].message.contains("must not be empty"));
    }

    #[test]
    fn rejects_exclude_without_wildcard() {
        let found =
            issues("[[keymap]]\napplication = [\"notepad.exe\"]\nexclude = [\"zed.exe\"]\n");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("requires application"));
    }

    #[test]
    fn rejects_modifier_target_on_bare_rule() {
        let found =
            issues("[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"CapsLock\" = \"C-a\"\n");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("may not have modifiers"));
        assert_eq!(found[0].line, 4);
    }

    #[test]
    fn rejects_macro_target_on_bare_rule() {
        let found = issues(
            "[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"CapsLock\" = [\"a\", \"b\"]\n",
        );
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("may not be a macro"));
    }

    #[test]
    fn rejects_modifier_key_as_input() {
        let found =
            issues("[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"LCtrl\" = \"a\"\n");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("cannot be a remap input"));
    }

    #[test]
    fn rejects_plain_and_prefix_conflict() {
        let found = issues(
            "[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"A-x\" = \"F2\"\n\"A-x h\" = \"C-a\"\n",
        );
        assert_eq!(found.len(), 1, "{found:?}");
        let message = &found[0].message;
        assert!(
            message.contains("sequence prefix"),
            "unexpected message: {message}"
        );
    }

    #[test]
    fn rejects_overlong_macro() {
        let found = issues(
            "[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"C-t\" = [\"a\",\"a\",\"a\",\"a\",\"a\",\"a\",\"a\",\"a\",\"a\"]\n",
        );
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("exceeds"));
    }

    #[test]
    fn rejects_rules_that_normalize_to_the_same_combo() {
        let found = issues(
            "[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"C-h\" = \"Back\"\n\"c-H\" = \"Delete\"\n",
        );
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("duplicates"));
    }
}
