//! Key notation parsing: `"C-h"`, `"A-x h"`, `"C-;"`, key names → VK codes.

use super::layout::{Layout, OEM_ALIASES, oem_alias, vk_for_alias};
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
    /// The character exists on this keyboard but only on a key's shifted
    /// face. Writing it plainly would have to mean "and hold Shift", which
    /// would make `C-@` and `C-S-2` the same rule on one keyboard and
    /// different rules on another (ADR 0063).
    #[error("`{character}` needs Shift on this keyboard; write `{write}` instead")]
    NeedsShift { character: char, write: String },
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
///
/// `layout` answers what the attached keyboard prints on its symbol keys; use
/// [`Layout::empty`] where none is available, and symbol characters are then
/// unknown key names.
pub fn parse_input_pattern(input: &str, layout: &Layout) -> Result<InputPattern, KeyParseError> {
    let mut strokes = input.split_whitespace();
    let Some(first) = strokes.next() else {
        return Err(KeyParseError::Empty);
    };
    let first = parse_key_combo(first, layout)?;
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
            Ok(InputPattern::Sequence(
                first,
                parse_key_combo(second, layout)?,
            ))
        }
    }
}

/// Parses notation like `"C-h"`, `"C-S-Enter"`, `"C-;"`, or `"Back"`
/// (config-spec §2). Prefixes and key names are case-insensitive.
pub fn parse_key_combo(input: &str, layout: &Layout) -> Result<KeyCombo, KeyParseError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(KeyParseError::Empty);
    }

    let mut mods = Mods::NONE;
    let mut rest = input;
    // Consume modifier prefixes left to right; whatever remains is the key
    // name. Splitting on every `-` instead would break if a future version
    // adds key names containing dashes.
    loop {
        // A lone `-` is the key itself, not a prefix with nothing after it:
        // `C--` is Ctrl plus the minus key on layouts that have one. Stopping
        // at one character is what lets the dash be both.
        if rest.chars().count() <= 1 {
            break;
        }
        let Some((head, tail)) = rest.split_once('-') else {
            break;
        };
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
    let vk = resolve_key_name(rest, layout)?;
    Ok(KeyCombo { mods, vk })
}

/// Win32 virtual-key code for a key name (config-spec §2), or `None` if this
/// keyboard has no such key.
#[must_use]
pub fn key_name_to_vk(name: &str, layout: &Layout) -> Option<u16> {
    resolve_key_name(name, layout).ok()
}

/// The key a name refers to, or why it refers to none.
///
/// Order matters: the layout is consulted last, so a keyboard that prints an
/// `f` or a digit on some exotic key can never shadow `F1` or `1`.
fn resolve_key_name(name: &str, layout: &Layout) -> Result<u16, KeyParseError> {
    if let Some(vk) = fixed_key_name_to_vk(name) {
        return Ok(vk);
    }
    // A single character that is not a letter or a digit is a symbol key, and
    // only the attached keyboard knows which one (ADR 0063).
    let mut chars = name.chars();
    if let (Some(character), None) = (chars.next(), chars.next()) {
        if let Some(vk) = layout.key_printing(character) {
            return Ok(vk);
        }
        if let Some(vk) = layout.shifted_key_printing(character) {
            return Err(KeyParseError::NeedsShift {
                character,
                write: format!("S-{}", shift_target_name(vk, layout)),
            });
        }
    }
    Err(KeyParseError::UnknownKey(name.to_string()))
}

/// How to spell the key that carries a shifted character, for the message
/// that tells the user what to write instead.
fn shift_target_name(vk: u16, layout: &Layout) -> String {
    // The engraved character, not the alias: the reader is being told which
    // key to press, and `S-;` points at it where `S-Oem1` sends them looking
    // it up.
    key_name(vk, layout).unwrap_or_else(|| format!("0x{vk:02X}"))
}

/// The part of the notation that means the same key on every keyboard.
fn fixed_key_name_to_vk(name: &str) -> Option<u16> {
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

    // `Oem1`, `OemMinus`, … — the layout-independent way to name a symbol key.
    if let Some(vk) = vk_for_alias(&lower) {
        return Some(vk);
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
        _ => return None,
    };
    Some(vk)
}

/// Canonical display name for a VK. The inverse of [`key_name_to_vk`] for
/// named keys; unknown codes fall back to hex.
pub fn vk_display_name(vk: u16) -> String {
    vk_config_name(vk).unwrap_or_else(|| format!("0x{vk:02X}"))
}

/// The name the config file uses for a VK, or `None` when the key has none.
///
/// Split out from [`vk_display_name`] so a caller can tell "this key is
/// called Down" from "this key has no name yet" — the hex fallback answers
/// the second case with something nobody can read, and the log needs to say
/// more than that about a key the user just pressed (ADR 0058).
pub fn vk_config_name(vk: u16) -> Option<String> {
    let name = match vk {
        0x41..=0x5A => char::from(b'a' + (vk - 0x41) as u8).to_string(),
        0x30..=0x39 => char::from(b'0' + (vk - 0x30) as u8).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        0x08 => "Back".to_string(),
        0x09 => "Tab".to_string(),
        0x0D => "Enter".to_string(),
        0x14 => "CapsLock".to_string(),
        0x1B => "Esc".to_string(),
        0x20 => "Space".to_string(),
        0x21 => "PageUp".to_string(),
        0x22 => "PageDown".to_string(),
        0x23 => "End".to_string(),
        0x24 => "Home".to_string(),
        0x25 => "Left".to_string(),
        0x26 => "Up".to_string(),
        0x27 => "Right".to_string(),
        0x28 => "Down".to_string(),
        0x2D => "Insert".to_string(),
        0x2E => "Delete".to_string(),
        0x5B => "LWin".to_string(),
        0x5C => "RWin".to_string(),
        0x5D => "Apps".to_string(),
        0xA0 => "LShift".to_string(),
        0xA1 => "RShift".to_string(),
        0xA2 => "LCtrl".to_string(),
        0xA3 => "RCtrl".to_string(),
        0xA4 => "LAlt".to_string(),
        0xA5 => "RAlt".to_string(),
        // A symbol key always has *a* name now, even without a layout: the
        // alias. Its character face is better where one is known, which is
        // why `Layout`-aware callers ask `key_name` instead (ADR 0063).
        _ => return oem_alias(vk).map(str::to_string),
    };
    Some(name)
}

/// The name to show for a key on *this* keyboard: the character the layout
/// prints on it, when it prints one, and the layout-independent name
/// otherwise.
///
/// This is what the log and the settings window use. The plain
/// [`vk_config_name`] cannot answer it — it has no keyboard to ask, so it
/// falls back to `Oem1` where the reader wants to see `;`.
#[must_use]
pub fn key_name(vk: u16, layout: &Layout) -> Option<String> {
    // Only the symbol keys are the layout's business. Asking it about `a`
    // would let a malformed snapshot rename the keys that never move.
    if oem_alias(vk).is_some()
        && let Some(face) = layout.face(vk)
    {
        return Some(face.to_string());
    }
    vk_config_name(vk)
}

/// Every canonical special-key name, in the order `vk_display_name` spells
/// them. Feeds the "did you mean" suggestion and the settings window's
/// key-name reference (B3) — one list, so neither can drift from the parser.
/// Letters, digits and F-keys are described categorically by the UI rather
/// than enumerated.
pub const SPECIAL_KEY_NAMES: &[&str] = &[
    "Back", "Tab", "Enter", "CapsLock", "Esc", "Space", "PageUp", "PageDown", "End", "Home",
    "Left", "Up", "Right", "Down", "Insert", "Delete", "LWin", "RWin", "Apps", "LShift", "RShift",
    "LCtrl", "RCtrl", "LAlt", "RAlt",
];

/// The closest known key name to a misspelling, when it is close enough to
/// be a plausible slip (edit distance ≤ 2): `"Bak"` → `"Back"`. `None` for
/// something too far from every name — a wild guess helps nobody.
pub fn suggest_key_name(unknown: &str) -> Option<&'static str> {
    // A single symbol is never a misspelling of a word: it is a key this
    // keyboard does not have. Without this, `;` is two edits from `Up` and
    // the reader is told to press an arrow key (ADR 0063).
    let mut chars = unknown.chars();
    if let (Some(character), None) = (chars.next(), chars.next())
        && !character.is_alphanumeric()
    {
        return None;
    }
    let lower = unknown.to_ascii_lowercase();
    SPECIAL_KEY_NAMES
        .iter()
        .chain(OEM_ALIASES.iter().map(|(_, alias)| alias))
        .map(|name| (edit_distance(&lower, &name.to_ascii_lowercase()), *name))
        .filter(|(distance, _)| *distance <= 2)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, name)| name)
}

/// Plain Levenshtein over bytes — key names are ASCII, inputs that are not
/// simply measure as further away, which is the right answer anyway.
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (row, &from) in a.iter().enumerate() {
        let mut current = vec![row + 1];
        for (column, &to) in b.iter().enumerate() {
            let substitute = previous[column] + usize::from(from != to);
            let insert = current[column] + 1;
            let delete = previous[column + 1] + 1;
            current.push(substitute.min(insert).min(delete));
        }
        previous = current;
    }
    previous[b.len()]
}

/// Side-specific modifier VKs (Shift/Ctrl/Alt/Win). These cannot be remap
/// *inputs*: the hook consumes them for chord-state tracking and never looks
/// them up, so config validation rejects them early instead of letting such
/// rules silently never fire.
pub fn is_modifier_vk(vk: u16) -> bool {
    matches!(vk, 0xA0..=0xA5 | 0x5B | 0x5C)
}
