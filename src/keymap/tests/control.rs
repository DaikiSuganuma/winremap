//! Tests for `keymap::control` (moved out of the module file, guidelines §5).

use super::{jp_106, us_101};
use crate::keymap::{ControlCode, Layout, control_code, parse_key_combo};

fn code(notation: &str) -> Option<ControlCode> {
    let layout = us_101();
    control_code(parse_key_combo(notation, &layout).expect("parses"), &layout)
}

fn code_on(notation: &str, layout: &Layout) -> Option<ControlCode> {
    control_code(parse_key_combo(notation, layout).expect("parses"), layout)
}

/// The pair this project exists for: the chord and the key it is remapped
/// to both carry BS, which is what says the remap delivers what Backspace
/// delivers (project brief §1.1).
#[test]
fn the_founding_pair_reads_the_same() {
    assert_eq!(code("C-h").map(|c| c.byte), Some(0x08));
    assert_eq!(code("Back").map(|c| c.byte), Some(0x08));
    assert_eq!(code("C-h").map(|c| c.name), Some("BS"));
}

#[test]
fn control_letters_are_the_low_five_bits() {
    assert_eq!(code("C-a").map(|c| c.byte), Some(0x01));
    assert_eq!(code("C-c").map(|c| c.byte), Some(0x03));
    assert_eq!(code("C-z").map(|c| c.byte), Some(0x1A));
    // Shift is not part of the answer.
    assert_eq!(code("C-S-c").map(|c| c.byte), Some(0x03));
}

#[test]
fn the_named_keys_carry_their_own_codes() {
    assert_eq!(code("Tab").map(|c| c.name), Some("HT"));
    assert_eq!(code("Enter").map(|c| c.name), Some("CR"));
    assert_eq!(code("Esc").map(|c| c.name), Some("ESC"));
    // The two that differ under Ctrl.
    assert_eq!(
        code("C-Back").map(|c| (c.byte, c.name)),
        Some((0x7F, "DEL"))
    );
    assert_eq!(
        code("C-Enter").map(|c| (c.byte, c.name)),
        Some((0x0A, "LF"))
    );
}

/// Invariant 6's boundary, as a test: what the user typed never turns
/// into a code in the log. Only keys that carry a control code do.
#[test]
fn printable_keys_carry_nothing() {
    for notation in ["a", "z", "1", "Space", "F1", "Home", "Down", "S-a"] {
        assert_eq!(code(notation), None, "{notation} must report no code");
    }
}

/// Alt and Win make a command, not a character.
#[test]
fn accelerators_carry_nothing() {
    for notation in ["A-h", "W-h", "C-A-h", "A-Back"] {
        assert_eq!(code(notation), None, "{notation} must report no code");
    }
}

/// The six ADR 0056 left out. `Ctrl+[` is ESC — the reason a terminal
/// user reaches for it at all.
#[test]
fn symbol_keys_carry_the_rest_of_the_c0_block() {
    assert_eq!(code("C-[").map(|c| (c.byte, c.name)), Some((0x1B, "ESC")));
    assert_eq!(code("C-\\").map(|c| (c.byte, c.name)), Some((0x1C, "FS")));
    assert_eq!(code("C-]").map(|c| (c.byte, c.name)), Some((0x1D, "GS")));
}

/// On a US keyboard three of the six are on shifted faces, and the chord
/// really does send the code — so Shift is read here, unlike for letters.
#[test]
fn shifted_faces_are_consulted_for_symbols() {
    assert_eq!(code("C-S-2").map(|c| (c.byte, c.name)), Some((0x00, "NUL")));
    assert_eq!(code("C-S-6").map(|c| (c.byte, c.name)), Some((0x1E, "RS")));
    assert_eq!(
        code("C-S-OemMinus").map(|c| (c.byte, c.name)),
        Some((0x1F, "US"))
    );
}

/// The same code, on a keyboard where it sits on a different key. This is
/// what a static table could not have got right (ADR 0063).
#[test]
fn the_same_code_follows_the_keyboard() {
    let jp = jp_106();
    // `[` is VK_OEM_4 on both layouts here, but `@` and `^` are not:
    // unshifted OEM keys on JP, shifted digits on US.
    assert_eq!(code_on("C-[", &jp).map(|c| c.byte), Some(0x1B));
    assert_eq!(
        code_on("C-@", &jp).map(|c| (c.byte, c.name)),
        Some((0x00, "NUL"))
    );
    assert_eq!(
        code_on("C-^", &jp).map(|c| (c.byte, c.name)),
        Some((0x1E, "RS"))
    );
    // ...and the US spelling of the same two keys is not even parseable
    // there, which is the notation being honest about the keyboard.
    assert_eq!(code_on("C-Oem3", &jp).map(|c| c.byte), Some(0x00));
}

/// Ctrl plus a symbol that carries no code stays silent, so the log does
/// not sprout numbers for ordinary punctuation.
#[test]
fn other_symbols_carry_nothing() {
    for notation in ["C-;", "C-,", "C-.", "C-/", "C-="] {
        assert_eq!(code(notation), None, "{notation} must report no code");
    }
}

/// Without Ctrl there is no control code, symbol key or not.
#[test]
fn a_symbol_alone_carries_nothing() {
    for notation in ["[", "\\", "S-2"] {
        assert_eq!(code(notation), None, "{notation} must report no code");
    }
}
