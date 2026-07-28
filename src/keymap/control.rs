//! The ASCII control code a key combination carries (ADR 0056).
//!
//! This project started from a key that looked right and behaved wrong:
//! Ctrl+H sends `0x08` (BS) while the Backspace key, in a terminal, deletes
//! by a different route — and the log said `C-h` and `Back` without ever
//! saying what either one sends (project brief §1.1).
//!
//! What is answered here is **only the C0 control codes**, and only the ones
//! that do not depend on the keyboard layout (owner decision 2026-07-29).
//! Printable characters are deliberately absent: `a` → `0x61` on every line
//! would turn the log into a record of what was typed, which invariant 6
//! forbids. A control code is a property of the key rather than of the text —
//! `C-h` is BS whatever the document says.
//!
//! Derived by table rather than by `ToUnicode`, because:
//!
//! - it is a pure function, so the cases that matter are unit-tested on
//!   headless CI rather than only on a Windows desktop;
//! - `ToUnicode` mutates the calling thread's dead-key state, which is a
//!   documented way to corrupt the next keystroke the user types — too high a
//!   price for a log line;
//! - for the set below the answer is layout-independent. Ctrl+letter is the
//!   letter's low five bits on every layout. The layout-dependent controls
//!   (Ctrl+`[`, Ctrl+`\`) are left out, and those keys have no config name
//!   yet anyway.

use super::{KeyCombo, Mods};

/// A control code and the mnemonic it is known by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ControlCode {
    pub byte: u8,
    /// `BS`, `CR`, `ESC` … The abbreviation is what makes a log line
    /// readable: `0x08` alone means nothing to most people.
    pub name: &'static str,
}

/// The control code `combo` carries, or `None` when it carries none.
///
/// `None` covers printable keys (`a`, `1`), keys that produce no character
/// (`F1`, `Home`), and every combination involving Alt or Win — those reach
/// an application as a command, not as a character.
pub fn control_code(combo: KeyCombo) -> Option<ControlCode> {
    // Alt and Win turn a key into an accelerator; nothing is sent as text,
    // so there is no code to report.
    if combo.mods.contains(Mods::ALT) || combo.mods.contains(Mods::WIN) {
        return None;
    }
    let ctrl = combo.mods.contains(Mods::CTRL);
    let byte = match (ctrl, combo.vk) {
        // Ctrl+letter is the letter's low five bits: Ctrl+A → 0x01 … Ctrl+Z
        // → 0x1A. Shift does not change it, which is why it is not consulted.
        (true, 0x41..=0x5A) => combo.vk as u8 - 0x40,
        // Ctrl+Backspace is DEL, the other half of the founding problem.
        (true, 0x08) => 0x7F,
        // Ctrl+Enter is LF where plain Enter is CR — the difference behind
        // half the "why did my macro insert a blank line" reports.
        (true, 0x0D) => 0x0A,
        (_, 0x08) => 0x08,
        (_, 0x09) => 0x09,
        (_, 0x0D) => 0x0D,
        (_, 0x1B) => 0x1B,
        _ => return None,
    };
    Some(ControlCode {
        byte,
        name: control_name(byte)?,
    })
}

/// The C0 mnemonic for a byte, or `None` if it is not a control code.
fn control_name(byte: u8) -> Option<&'static str> {
    if byte == 0x7F {
        return Some("DEL");
    }
    C0_NAMES.get(usize::from(byte)).copied()
}

/// The C0 set, in order, as ISO/IEC 6429 names them.
const C0_NAMES: [&str; 32] = [
    "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT", "FF", "CR",
    "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC",
    "FS", "GS", "RS", "US",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::parse_key_combo;

    fn code(notation: &str) -> Option<ControlCode> {
        control_code(parse_key_combo(notation).expect("parses"))
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
}
