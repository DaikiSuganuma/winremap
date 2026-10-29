//! Tests for the human-words renderings (moved out of the module file, guidelines §5).

use super::*;
use winremap::keymap::{Layout, parse_input_pattern, parse_key_combo};

/// None of these read a symbol key, so no keyboard is needed to say what
/// they mean.
fn no_keyboard() -> Layout {
    Layout::empty()
}

// LANG is never initialized under test, so everything renders English.
#[test]
fn renders_notation_in_human_words() {
    let combo = parse_key_combo("C-S-h", &no_keyboard()).expect("parses");
    assert_eq!(combo_human(&combo), "Ctrl + Shift + h");

    let sequence = parse_input_pattern("A-x h", &no_keyboard()).expect("parses");
    assert_eq!(input_human(&sequence), "Alt + x, then h (two strokes)");

    let single = parse_input_pattern("C-h", &no_keyboard()).expect("parses");
    assert_eq!(input_human(&single), "Ctrl + h");

    let steps = [
        parse_key_combo("C-Right", &no_keyboard()).expect("parses"),
        parse_key_combo("C-Left", &no_keyboard()).expect("parses"),
    ];
    assert_eq!(output_human(&steps), "Ctrl + Right → Ctrl + Left (macro)");
    assert_eq!(output_human(&steps[..1]), "Ctrl + Right");
}
