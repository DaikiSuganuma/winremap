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
//! - for the letters and the named keys the answer is layout-independent.
//!   Ctrl+letter is the letter's low five bits on every layout.
//!
//! The remaining six — Ctrl+`@`, `[`, `\`, `]`, `^`, `_` — *are* layout
//! dependent, and ADR 0056 left them out because "those keys have no config
//! name yet anyway". They have one now (ADR 0063), so they are answered here
//! by asking the layout what the key prints. That is the only part of this
//! module that depends on the attached keyboard.

use super::layout::Layout;
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
pub fn control_code(combo: KeyCombo, layout: &Layout) -> Option<ControlCode> {
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
        (true, vk) => symbol_control_byte(combo, vk, layout)?,
        _ => return None,
    };
    Some(ControlCode {
        byte,
        name: control_name(byte)?,
    })
}

/// The C0 code for Ctrl plus one of the six symbol keys that carry one.
///
/// Unlike the letters, *which* key this is differs by keyboard: `Ctrl+[` is
/// ESC on a US layout, and the key that prints `[` is a different one on a JP
/// layout. So the character is asked for rather than assumed.
///
/// Shift is consulted here — and only here — because on many layouts the
/// character that carries the code is on the shifted face: `^` is Shift+6 on
/// a US keyboard, and `C-S-6` really does send RS.
fn symbol_control_byte(combo: KeyCombo, vk: u16, layout: &Layout) -> Option<u8> {
    let face = if combo.mods.contains(Mods::SHIFT) {
        layout.shifted_face(vk).or_else(|| layout.face(vk))?
    } else {
        layout.face(vk)?
    };
    // The C0 block continues past Z with exactly these six, in ASCII order:
    // @ = NUL, [ = ESC, \ = FS, ] = GS, ^ = RS, _ = US.
    match face {
        '@' => Some(0x00),
        '[' => Some(0x1B),
        '\\' => Some(0x1C),
        ']' => Some(0x1D),
        '^' => Some(0x1E),
        '_' => Some(0x1F),
        _ => None,
    }
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
