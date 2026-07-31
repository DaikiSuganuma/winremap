use std::collections::HashMap;

use super::*;

/// A US 101/104 keyboard, transcribed from the readings taken on the
/// development machine (v0.7 notes, 2026-07-31).
///
/// Two keyboards are kept here rather than one so the layout-dependent half
/// of the notation is tested from both sides in the same process. Without
/// that, "works on my keyboard" is the only assurance there is — and the
/// development machine is US, so the Japanese case would go unexercised
/// exactly where it matters most (ADR 0063).
pub(crate) fn us_101() -> Layout {
    Layout::new(
        vec![
            (0xBA, ';'),
            (0xBB, '='),
            (0xBC, ','),
            (0xBD, '-'),
            (0xBE, '.'),
            (0xBF, '/'),
            (0xC0, '`'),
            (0xDB, '['),
            (0xDC, '\\'),
            (0xDD, ']'),
            (0xDE, '\''),
            // Measured: a US layout prints `\` on both VK_OEM_5 and
            // VK_OEM_102. The lower code wins for `\`; VK_OEM_102 stays
            // reachable as `Oem102`.
            (0xE2, '\\'),
        ],
        vec![
            (':', 0xBA),
            ('+', 0xBB),
            ('<', 0xBC),
            ('_', 0xBD),
            ('>', 0xBE),
            ('?', 0xBF),
            ('~', 0xC0),
            ('{', 0xDB),
            ('|', 0xDC),
            ('}', 0xDD),
            ('"', 0xDE),
            // The number row, where a US keyboard keeps `@` and `^`.
            (')', 0x30),
            ('!', 0x31),
            ('@', 0x32),
            ('#', 0x33),
            ('$', 0x34),
            ('%', 0x35),
            ('^', 0x36),
            ('&', 0x37),
            ('*', 0x38),
            ('(', 0x39),
        ],
    )
}

/// A Japanese 106/109 (JIS) keyboard. Not measured — no such keyboard was
/// attached — but transcribed from the layout's published key faces and
/// recorded as the expectation to check when one is (v0.7 notes §4).
pub(crate) fn jp_106() -> Layout {
    Layout::new(
        vec![
            (0xBA, ':'),
            (0xBB, ';'),
            (0xBC, ','),
            (0xBD, '-'),
            (0xBE, '.'),
            (0xBF, '/'),
            (0xC0, '@'),
            (0xDB, '['),
            (0xDC, '¥'),
            (0xDD, ']'),
            (0xDE, '^'),
            // The ろ key, which a US keyboard does not have at all.
            (0xE2, '\\'),
        ],
        vec![
            ('*', 0xBA),
            ('+', 0xBB),
            ('<', 0xBC),
            ('=', 0xBD),
            ('>', 0xBE),
            ('?', 0xBF),
            ('`', 0xC0),
            ('{', 0xDB),
            ('|', 0xDC),
            ('}', 0xDD),
            ('~', 0xDE),
            ('_', 0xE2),
            ('!', 0x31),
            ('"', 0x32),
            ('#', 0x33),
            ('$', 0x34),
            ('%', 0x35),
            ('&', 0x36),
            ('\'', 0x37),
            ('(', 0x38),
            (')', 0x39),
        ],
    )
}

fn combo(spec: &str) -> KeyCombo {
    parse_key_combo(spec, &us_101()).unwrap()
}

fn parse(spec: &str) -> Result<KeyCombo, KeyParseError> {
    parse_key_combo(spec, &us_101())
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
    assert_eq!(parse(""), Err(KeyParseError::Empty));
    assert_eq!(parse("  "), Err(KeyParseError::Empty));
    assert_eq!(parse("C-"), Err(KeyParseError::MissingKey));
    assert_eq!(
        parse("X-h"),
        Err(KeyParseError::UnknownModifier("X".to_string()))
    );
    assert_eq!(
        parse("C-c-h"),
        Err(KeyParseError::DuplicateModifier("c".to_string()))
    );
    assert_eq!(
        parse("C-Bogus"),
        Err(KeyParseError::UnknownKey("Bogus".to_string()))
    );
    assert_eq!(
        parse("F25"),
        Err(KeyParseError::UnknownKey("F25".to_string()))
    );
}

#[test]
fn parses_input_patterns() {
    let us = us_101();
    assert_eq!(
        parse_input_pattern("C-h", &us),
        Ok(InputPattern::Single(combo("C-h")))
    );
    assert_eq!(
        parse_input_pattern("A-x h", &us),
        Ok(InputPattern::Sequence(combo("A-x"), combo("h")))
    );
    assert_eq!(
        parse_input_pattern("A-x C-s", &us),
        Ok(InputPattern::Sequence(combo("A-x"), combo("C-s")))
    );
    assert_eq!(
        parse_input_pattern("A-x h k", &us),
        Err(KeyParseError::TooManyStrokes)
    );
    // Prefixes must be chords, or plain typing would get swallowed.
    assert_eq!(
        parse_input_pattern("x h", &us),
        Err(KeyParseError::UnmodifiedPrefix)
    );
    // A symbol works as either stroke of a sequence.
    assert_eq!(
        parse_input_pattern("A-x ;", &us),
        Ok(InputPattern::Sequence(combo("A-x"), combo(";")))
    );
}

fn table_with(keymap: Keymap) -> RemapTable {
    RemapTable {
        keymaps: vec![keymap],
        macro_delay_ms: 0,
        ime_indicator: Default::default(),
        macro_record: None,
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
fn key_combo_displays_in_config_notation() {
    assert_eq!(combo("C-S-h").to_string(), "C-S-h");
    assert_eq!(combo("Back").to_string(), "Back");
    assert_eq!(combo("A-F5").to_string(), "A-F5");
    assert_eq!(combo("W-2").to_string(), "W-2");
    // Round-trip: the displayed form parses back to the same combo.
    for spec in ["C-h", "A-x", "C-A-S-Delete", "PageUp"] {
        assert_eq!(parse(&combo(spec).to_string()), Ok(combo(spec)));
    }
    // A symbol key displays as its layout-independent alias, because Display
    // has no keyboard to ask — and that alias parses back to the same key.
    assert_eq!(combo("C-;").to_string(), "C-Oem1");
    assert_eq!(parse("C-Oem1"), Ok(combo("C-;")));
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

#[test]
fn suggests_the_nearest_key_name_for_a_slip() {
    assert_eq!(suggest_key_name("Bak"), Some("Back"));
    assert_eq!(suggest_key_name("Entre"), Some("Enter"));
    assert_eq!(suggest_key_name("capslok"), Some("CapsLock"));
    // Too far from anything: no wild guesses.
    assert_eq!(suggest_key_name("qqqqqq"), None);
}

#[test]
fn special_key_names_all_parse() {
    let us = us_101();
    for name in SPECIAL_KEY_NAMES {
        assert!(key_name_to_vk(name, &us).is_some(), "{name} should parse");
    }
}

#[test]
fn symbol_keys_parse_by_the_character_on_them() {
    assert_eq!(combo("C-;").vk, 0xBA);
    assert_eq!(combo("C-/").vk, 0xBF);
    assert_eq!(combo("C-[").vk, 0xDB);
    // A bare symbol is a key like any other.
    assert_eq!(combo(";").vk, 0xBA);
    assert_eq!(combo(";").mods, Mods::NONE);
}

/// The minus key, which the notation also uses as its modifier separator.
/// `C--` has to mean Ctrl plus that key, and `--` is not two modifiers.
#[test]
fn the_dash_key_is_reachable() {
    assert_eq!(combo("-").vk, 0xBD);
    assert_eq!(
        combo("C--"),
        KeyCombo {
            mods: Mods::CTRL,
            vk: 0xBD
        }
    );
    assert_eq!(
        combo("C-S--"),
        KeyCombo {
            mods: Mods::CTRL.with(Mods::SHIFT),
            vk: 0xBD
        }
    );
    // And the alias still names the same key.
    assert_eq!(combo("C-OemMinus"), combo("C--"));
}

/// The same rule text means a different physical key on a different keyboard
/// — which is the point of writing what is engraved (ADR 0063).
#[test]
fn the_character_follows_the_keyboard() {
    let jp = jp_106();
    assert_eq!(parse_key_combo("C-;", &jp).unwrap().vk, 0xBB);
    assert_eq!(combo("C-;").vk, 0xBA);
    // `@` is an ordinary key on a JIS keyboard...
    assert_eq!(parse_key_combo("C-@", &jp).unwrap().vk, 0xC0);
    // ...and `` ` `` is not one at all: it is Shift+@ there.
    assert_eq!(
        parse_key_combo("C-`", &jp),
        Err(KeyParseError::NeedsShift {
            character: '`',
            write: "S-@".to_string(),
        })
    );
}

/// The aliases mean the same key on every keyboard, which is what makes a
/// config portable between them.
#[test]
fn aliases_are_layout_independent() {
    let jp = jp_106();
    for (alias, vk) in [
        ("Oem1", 0xBA),
        ("OemPlus", 0xBB),
        ("Oem3", 0xC0),
        ("Oem102", 0xE2),
    ] {
        assert_eq!(combo(alias).vk, vk, "{alias} on US");
        assert_eq!(parse_key_combo(alias, &jp).unwrap().vk, vk, "{alias} on JP");
    }
    // Case-insensitive like every other key name.
    assert_eq!(combo("oem1"), combo("Oem1"));
}

/// A character that exists only on a shifted face is refused, with the
/// spelling that does work. Silently folding in the Shift would make `C-@`
/// and `C-S-2` the same rule here and different rules on a JIS keyboard.
#[test]
fn a_shifted_character_says_what_to_write_instead() {
    assert_eq!(
        parse("C-@"),
        Err(KeyParseError::NeedsShift {
            character: '@',
            write: "S-2".to_string(),
        })
    );
    assert_eq!(
        parse(":"),
        Err(KeyParseError::NeedsShift {
            character: ':',
            write: "S-;".to_string(),
        })
    );
    // The message is the part the reader acts on, so assert it too.
    assert_eq!(
        parse("C-@").unwrap_err().to_string(),
        "`@` needs Shift on this keyboard; write `S-2` instead"
    );
}

/// A character no key on this keyboard prints is simply unknown — and the
/// suggestion machinery keeps quiet rather than proposing an arrow key.
#[test]
fn a_character_this_keyboard_lacks_is_unknown() {
    assert_eq!(
        parse("C-¥"),
        Err(KeyParseError::UnknownKey("¥".to_string()))
    );
    assert_eq!(suggest_key_name("¥"), None);
    assert_eq!(suggest_key_name(";"), None);
    // Aliases join the suggestions, since they are words and can be misspelt.
    assert_eq!(suggest_key_name("Oem11"), Some("Oem1"));
}

/// Two keys print `\` on a US keyboard. The character resolves to one of
/// them, always the same one; the other is still reachable by its alias.
#[test]
fn a_character_on_two_keys_resolves_to_one_of_them() {
    assert_eq!(combo("\\").vk, 0xDC);
    assert_eq!(combo("Oem102").vk, 0xE2);
    assert_ne!(combo("\\"), combo("Oem102"));
}

/// A key that prints nothing has no character to write, only a name.
#[test]
fn a_key_with_no_face_is_reachable_by_alias_only() {
    assert_eq!(combo("Oem8").vk, 0xDF);
    assert_eq!(us_101().face(0xDF), None);
}

/// What a key is *called* follows the keyboard; what the parser calls it
/// when there is no keyboard to ask is the alias.
#[test]
fn key_names_prefer_the_engraved_character() {
    let (us, jp) = (us_101(), jp_106());
    assert_eq!(key_name(0xBA, &us).as_deref(), Some(";"));
    assert_eq!(key_name(0xBA, &jp).as_deref(), Some(":"));
    assert_eq!(key_name(0xBA, &Layout::empty()).as_deref(), Some("Oem1"));
    // Keys that do not move are never the layout's business.
    assert_eq!(key_name(0x41, &jp).as_deref(), Some("a"));
    assert_eq!(key_name(0x28, &jp).as_deref(), Some("Down"));
    // A symbol key with no face still has its alias.
    assert_eq!(key_name(0xDF, &us).as_deref(), Some("Oem8"));
}

/// Every symbol key the keyboard has is offered to the settings window with
/// both spellings, so the reader can see which alias is which key.
#[test]
fn symbol_keys_are_listed_with_both_spellings() {
    let listed = us_101().symbol_keys();
    assert!(listed.contains(&(';', "Oem1")));
    assert!(listed.contains(&('-', "OemMinus")));
    // The key with no face is absent: there is nothing to show for it.
    assert!(!listed.iter().any(|(_, alias)| *alias == "Oem8"));
    assert!(Layout::empty().symbol_keys().is_empty());
}
