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

use crate::keymap::{AppFilter, Keymap, RemapTable, parse_key_combo};

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
    remap: BTreeMap<Spanned<String>, Spanned<String>>,
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
        let apps = compile_app_filter(&name, &raw_keymap.application, source, &mut issues);

        let mut exact = HashMap::new();
        let mut bare = HashMap::new();
        for (lhs, rhs) in &raw_keymap.remap {
            let input = match parse_key_combo(lhs.get_ref()) {
                Ok(combo) => combo,
                Err(e) => {
                    issues.push(issue_at(source, lhs.span().start, &name, &e.to_string()));
                    continue;
                }
            };
            let target = match parse_key_combo(rhs.get_ref()) {
                Ok(combo) => combo,
                Err(e) => {
                    issues.push(issue_at(source, rhs.span().start, &name, &e.to_string()));
                    continue;
                }
            };

            let duplicate = if input.mods.is_empty() {
                // Bare rules leave physical modifiers untouched, so a
                // modifier on the target would silently do nothing useful;
                // reject it until the sender can synthesize chords (v0.2).
                if !target.mods.is_empty() {
                    issues.push(issue_at(
                        source,
                        rhs.span().start,
                        &name,
                        &format!(
                            "target `{}` may not have modifiers in a bare-key rule (v0.1 limitation)",
                            rhs.get_ref()
                        ),
                    ));
                    continue;
                }
                bare.insert(input.vk, target).is_some()
            } else {
                exact.insert(input, target).is_some()
            };
            // TOML rejects duplicate literal keys, but distinct spellings
            // ("C-h" vs "c-H") normalize to the same combo; catch those here.
            if duplicate {
                issues.push(issue_at(
                    source,
                    lhs.span().start,
                    &name,
                    &format!(
                        "`{}` duplicates an earlier rule for the same key",
                        lhs.get_ref()
                    ),
                ));
            }
        }
        keymaps.push(Keymap {
            name,
            apps,
            exact,
            bare,
        });
    }

    if issues.is_empty() {
        Ok(RemapTable { keymaps })
    } else {
        Err(ConfigError::Invalid(issues))
    }
}

fn compile_app_filter(
    name: &str,
    application: &Spanned<Vec<String>>,
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
    if entries.iter().any(|e| e == "*") {
        // Mixing "*" with concrete names would make the section's scope
        // ambiguous, so it is rejected rather than silently widened.
        if entries.len() > 1 {
            issues.push(issue_at(
                source,
                offset,
                name,
                "wildcard \"*\" cannot be combined with specific application names",
            ));
        }
        return AppFilter::All;
    }
    if let Some(empty) = entries.iter().find(|e| e.trim().is_empty()) {
        issues.push(issue_at(
            source,
            offset,
            name,
            &format!("empty application name `{empty}`"),
        ));
    }
    AppFilter::Names(entries.clone())
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
    fn rejects_modifier_target_on_bare_rule() {
        let found =
            issues("[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"CapsLock\" = \"C-a\"\n");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("bare-key rule"));
        assert_eq!(found[0].line, 4);
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
