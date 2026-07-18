use std::collections::HashMap;

use super::*;

fn combo(spec: &str) -> KeyCombo {
    parse_key_combo(spec).unwrap()
}

#[test]
fn parses_bare_keys() {
    assert_eq!(
        combo("Back"),
        KeyCombo {
            mods: Mods::NONE,
            vk: 0x08
        }
    );
    assert_eq!(combo("a").vk, 0x41);
    assert_eq!(combo("Z").vk, 0x5A);
    assert_eq!(combo("0").vk, 0x30);
    assert_eq!(combo("F1").vk, 0x70);
    assert_eq!(combo("f24").vk, 0x87);
}

#[test]
fn parses_modifier_prefixes() {
    assert_eq!(
        combo("C-h"),
        KeyCombo {
            mods: Mods::CTRL,
            vk: 0x48
        }
    );
    // Prefix order and letter case must not matter.
    assert_eq!(combo("s-c-A-w-F5"), combo("C-A-S-W-f5"));
}

#[test]
fn accepts_aliases() {
    assert_eq!(combo("Backspace"), combo("Back"));
    assert_eq!(combo("BS"), combo("Back"));
    assert_eq!(combo("Return"), combo("Enter"));
    assert_eq!(combo("Escape"), combo("Esc"));
    assert_eq!(combo("Del"), combo("Delete"));
    assert_eq!(combo("PgUp"), combo("PageUp"));
}

#[test]
fn rejects_invalid_notation() {
    assert_eq!(parse_key_combo(""), Err(KeyParseError::Empty));
    assert_eq!(parse_key_combo("  "), Err(KeyParseError::Empty));
    assert_eq!(parse_key_combo("C-"), Err(KeyParseError::MissingKey));
    assert_eq!(
        parse_key_combo("X-h"),
        Err(KeyParseError::UnknownModifier("X".to_string()))
    );
    assert_eq!(
        parse_key_combo("C-c-h"),
        Err(KeyParseError::DuplicateModifier("c".to_string()))
    );
    assert_eq!(
        parse_key_combo("C-Bogus"),
        Err(KeyParseError::UnknownKey("Bogus".to_string()))
    );
    assert_eq!(
        parse_key_combo("F25"),
        Err(KeyParseError::UnknownKey("F25".to_string()))
    );
}

#[test]
fn parses_input_patterns() {
    assert_eq!(
        parse_input_pattern("C-h"),
        Ok(InputPattern::Single(combo("C-h")))
    );
    assert_eq!(
        parse_input_pattern("A-x h"),
        Ok(InputPattern::Sequence(combo("A-x"), combo("h")))
    );
    assert_eq!(
        parse_input_pattern("A-x C-s"),
        Ok(InputPattern::Sequence(combo("A-x"), combo("C-s")))
    );
    assert_eq!(
        parse_input_pattern("A-x h k"),
        Err(KeyParseError::TooManyStrokes)
    );
    // Prefixes must be chords, or plain typing would get swallowed.
    assert_eq!(
        parse_input_pattern("x h"),
        Err(KeyParseError::UnmodifiedPrefix)
    );
}

fn table_with(keymap: Keymap) -> RemapTable {
    RemapTable {
        keymaps: vec![keymap],
    }
}

fn empty_keymap() -> Keymap {
    Keymap {
        name: "t".to_string(),
        apps: AppFilter::All {
            exclude: Vec::new(),
        },
        exact: HashMap::new(),
        bare: HashMap::new(),
        seqs: HashMap::new(),
    }
}

#[test]
fn exact_rules_require_exact_modifier_state() {
    let mut keymap = empty_keymap();
    keymap
        .exact
        .insert(combo("C-h"), Output::Chord(combo("Back")));
    let table = table_with(keymap);
    assert!(table.resolve("x.exe", combo("C-h")).is_some());
    // Extra Shift must not trigger the C-h rule (ADR 0004).
    assert!(table.resolve("x.exe", combo("C-S-h")).is_none());
    assert!(table.resolve("x.exe", combo("h")).is_none());
}

#[test]
fn bare_rules_ignore_modifier_state() {
    let mut keymap = empty_keymap();
    keymap.bare.insert(combo("CapsLock").vk, combo("LCtrl").vk);
    let table = table_with(keymap);
    assert_eq!(
        table.resolve("x.exe", combo("C-CapsLock")),
        Some(Resolution::KeyOnly(combo("LCtrl").vk))
    );
}

#[test]
fn excluded_apps_do_not_match_global_keymaps() {
    let mut keymap = empty_keymap();
    keymap.apps = AppFilter::All {
        exclude: vec!["Zed.exe".to_string()],
    };
    keymap
        .exact
        .insert(combo("C-h"), Output::Chord(combo("Back")));
    let table = table_with(keymap);
    assert!(table.resolve("notepad.exe", combo("C-h")).is_some());
    // Exclusion is case-insensitive like all exe matching.
    assert!(table.resolve("zed.exe", combo("C-h")).is_none());
}

#[test]
fn sequences_resolve_via_prefix_then_second_stroke() {
    let mut keymap = empty_keymap();
    keymap.seqs.insert(
        combo("A-x"),
        HashMap::from([(combo("u"), Output::Chord(combo("C-z")))]),
    );
    let table = table_with(keymap);
    assert_eq!(
        table.resolve("x.exe", combo("A-x")),
        Some(Resolution::Prefix)
    );
    assert_eq!(
        table.resolve_second("x.exe", combo("A-x"), combo("u")),
        Some(&Output::Chord(combo("C-z")))
    );
    assert_eq!(
        table.resolve_second("x.exe", combo("A-x"), combo("q")),
        None
    );
}
