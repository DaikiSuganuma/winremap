//! Key notation parsing: `"C-h"`, `"A-x h"`, key names → VK codes.

use super::{KeyCombo, Mods};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum KeyParseError {
    #[error("empty key specification")]
    Empty,
    #[error("missing key name after modifier prefix")]
    MissingKey,
    #[error("unknown modifier prefix `{0}-` (expected C-, A-, S-, or W-)")]
    UnknownModifier(String),
    #[error("duplicate modifier prefix `{0}-`")]
    DuplicateModifier(String),
    #[error("unknown key name `{0}`")]
    UnknownKey(String),
    #[error("too many strokes (at most 2, e.g. `A-x h`)")]
    TooManyStrokes,
    #[error("the first stroke of a sequence must include a modifier")]
    UnmodifiedPrefix,
}

/// A rule's input: a single chord, or a two-stroke sequence (`"A-x h"`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputPattern {
    Single(KeyCombo),
    Sequence(KeyCombo, KeyCombo),
}

/// Parses a remap LHS: `"C-h"` or a whitespace-separated two-stroke
/// sequence like `"A-x h"` (config-spec §3.3, ADR 0013).
pub fn parse_input_pattern(input: &str) -> Result<InputPattern, KeyParseError> {
    let mut strokes = input.split_whitespace();
    let Some(first) = strokes.next() else {
        return Err(KeyParseError::Empty);
    };
    let first = parse_key_combo(first)?;
    match strokes.next() {
        None => Ok(InputPattern::Single(first)),
        Some(second) => {
            if strokes.next().is_some() {
                return Err(KeyParseError::TooManyStrokes);
            }
            // An unmodified first stroke would turn a plain typing key into a
            // prefix that swallows the following keystroke; require a chord.
            if first.mods.is_empty() {
                return Err(KeyParseError::UnmodifiedPrefix);
            }
            Ok(InputPattern::Sequence(first, parse_key_combo(second)?))
        }
    }
}

/// Parses notation like `"C-h"`, `"C-S-Enter"`, or `"Back"` (config-spec §2).
/// Prefixes and key names are case-insensitive.
pub fn parse_key_combo(input: &str) -> Result<KeyCombo, KeyParseError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(KeyParseError::Empty);
    }

    let mut mods = Mods::NONE;
    let mut rest = input;
    // Consume modifier prefixes left to right; whatever remains is the key
    // name. Splitting on every `-` instead would break if a future version
    // adds key names containing dashes.
    while let Some((head, tail)) = rest.split_once('-') {
        let flag = if head.eq_ignore_ascii_case("c") {
            Mods::CTRL
        } else if head.eq_ignore_ascii_case("a") {
            Mods::ALT
        } else if head.eq_ignore_ascii_case("s") {
            Mods::SHIFT
        } else if head.eq_ignore_ascii_case("w") {
            Mods::WIN
        } else {
            return Err(KeyParseError::UnknownModifier(head.to_string()));
        };
        if mods.contains(flag) {
            return Err(KeyParseError::DuplicateModifier(head.to_string()));
        }
        mods = mods.with(flag);
        rest = tail;
    }

    if rest.is_empty() {
        return Err(KeyParseError::MissingKey);
    }
    let vk = key_name_to_vk(rest).ok_or_else(|| KeyParseError::UnknownKey(rest.to_string()))?;
    Ok(KeyCombo { mods, vk })
}

/// Win32 virtual-key code for a key name (config-spec §2), or `None` if the
/// name is not supported.
pub fn key_name_to_vk(name: &str) -> Option<u16> {
    let lower = name.to_ascii_lowercase();

    if lower.len() == 1 {
        let b = lower.as_bytes()[0];
        // VK codes for letters and digits equal their uppercase ASCII values.
        if b.is_ascii_lowercase() {
            return Some(u16::from(b) - u16::from(b'a') + 0x41);
        }
        if b.is_ascii_digit() {
            return Some(u16::from(b));
        }
    }

    if let Some(num) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=24).contains(&num)
    {
        return Some(0x70 + u16::from(num) - 1); // VK_F1..VK_F24
    }

    let vk: u16 = match lower.as_str() {
        "back" | "backspace" | "bs" => 0x08,
        "tab" => 0x09,
        "enter" | "return" => 0x0D,
        "capslock" => 0x14,
        "esc" | "escape" => 0x1B,
        "space" => 0x20,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "insert" | "ins" => 0x2D,
        "delete" | "del" => 0x2E,
        "lwin" => 0x5B,
        "rwin" => 0x5C,
        "apps" | "menu" => 0x5D,
        "lshift" => 0xA0,
        "rshift" => 0xA1,
        "lctrl" | "lcontrol" => 0xA2,
        "rctrl" | "rcontrol" => 0xA3,
        "lalt" => 0xA4,
        "ralt" => 0xA5,
        // TODO: OEM/punctuation keys — VK codes are layout-dependent (JP
        // keyboards differ), deferred with its own ADR.
        _ => return None,
    };
    Some(vk)
}

/// Side-specific modifier VKs (Shift/Ctrl/Alt/Win). These cannot be remap
/// *inputs*: the hook consumes them for chord-state tracking and never looks
/// them up, so config validation rejects them early instead of letting such
/// rules silently never fire.
pub fn is_modifier_vk(vk: u16) -> bool {
    matches!(vk, 0xA0..=0xA5 | 0x5B | 0x5C)
}
