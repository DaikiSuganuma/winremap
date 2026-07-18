//! Key notation parsing and remap resolution.
//!
//! This module is pure logic with no OS dependency so it runs under `cargo
//! test` on headless CI (project brief §9). Virtual-key codes are plain `u16`
//! values matching the Win32 `VK_*` constants, hardcoded here to avoid a
//! `windows` crate dependency in the testable core.

use std::collections::HashMap;

/// Modifier set as a bitflag. Hand-rolled instead of the `bitflags` crate to
/// keep dependencies minimal for such a tiny surface.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Self = Self(0);
    pub const CTRL: Self = Self(1);
    pub const ALT: Self = Self(1 << 1);
    pub const SHIFT: Self = Self(1 << 2);
    pub const WIN: Self = Self(1 << 3);

    #[must_use]
    pub fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A key plus the exact modifier set that goes with it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyCombo {
    pub mods: Mods,
    pub vk: u16,
}

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
    // name. Splitting on every `-` instead would break if v0.2 adds key names
    // containing dashes.
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
/// name is not supported in v0.1.
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
        // keyboards differ), deferred to v0.2 with its own ADR.
        _ => return None,
    };
    Some(vk)
}

/// Side-specific modifier VKs (Shift/Ctrl/Alt/Win). These cannot be remap
/// *inputs* in v0.1: the hook consumes them for chord-state tracking and
/// never looks them up, so config validation rejects them early instead of
/// letting such rules silently never fire.
pub fn is_modifier_vk(vk: u16) -> bool {
    matches!(vk, 0xA0..=0xA5 | 0x5B | 0x5C)
}

/// How the sender must treat a resolved remap (config-spec §3, ADR 0004).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RemapKind {
    /// Modifier-specified rule: the held modifiers are replaced by the
    /// target's modifiers (e.g. `C-h` sends a plain Backspace).
    Exact,
    /// Bare-key rule: only the key is substituted; physical modifiers are
    /// left untouched (e.g. CapsLock→LCtrl must work mid-chord).
    KeyOnly,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RemapAction {
    pub target: KeyCombo,
    pub kind: RemapKind,
}

/// Which processes a keymap applies to.
#[derive(Clone, Debug)]
pub enum AppFilter {
    /// `application = ["*"]`
    All,
    /// Exact exe names, matched case-insensitively.
    Names(Vec<String>),
}

impl AppFilter {
    /// Case-insensitive because Windows file names are; compares without
    /// allocating so it is safe to call from the hook callback.
    pub fn matches(&self, exe: &str) -> bool {
        match self {
            AppFilter::All => true,
            AppFilter::Names(names) => names.iter().any(|n| n.eq_ignore_ascii_case(exe)),
        }
    }

    pub fn is_global(&self) -> bool {
        matches!(self, AppFilter::All)
    }
}

/// One compiled `[[keymap]]` section.
#[derive(Debug)]
pub struct Keymap {
    pub name: String,
    pub apps: AppFilter,
    /// Rules whose input listed modifiers; matched on the full combo.
    pub exact: HashMap<KeyCombo, KeyCombo>,
    /// Bare-key rules; matched on the VK alone, ignoring held modifiers.
    pub bare: HashMap<u16, KeyCombo>,
}

impl Keymap {
    fn lookup(&self, input: KeyCombo) -> Option<RemapAction> {
        // Exact rules win over bare ones so an app can both swap a key
        // globally and still special-case a chord on it (ADR 0004).
        if let Some(&target) = self.exact.get(&input) {
            return Some(RemapAction {
                target,
                kind: RemapKind::Exact,
            });
        }
        if let Some(&target) = self.bare.get(&input.vk) {
            return Some(RemapAction {
                target,
                kind: RemapKind::KeyOnly,
            });
        }
        None
    }
}

/// The read-only structure the hook resolves events against. Built by
/// `config::parse_str` and swapped atomically on reload (ADR 0003).
#[derive(Debug)]
pub struct RemapTable {
    pub keymaps: Vec<Keymap>,
}

impl RemapTable {
    /// Resolves a key event for the foreground process `exe`.
    ///
    /// Runs inside the low-level hook: must not allocate or block (AGENTS.md
    /// invariant 2). `None` means "pass the event through unchanged".
    pub fn resolve(&self, exe: &str, input: KeyCombo) -> Option<RemapAction> {
        // Two passes so app-specific keymaps always beat `*` ones regardless
        // of definition order; within a pass, first match wins (ADR 0004).
        self.keymaps
            .iter()
            .filter(|k| !k.apps.is_global() && k.apps.matches(exe))
            .find_map(|k| k.lookup(input))
            .or_else(|| {
                self.keymaps
                    .iter()
                    .filter(|k| k.apps.is_global())
                    .find_map(|k| k.lookup(input))
            })
    }
}

#[cfg(test)]
mod tests {
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
    fn exact_rules_require_exact_modifier_state() {
        let keymap = Keymap {
            name: "t".to_string(),
            apps: AppFilter::All,
            exact: HashMap::from([(combo("C-h"), combo("Back"))]),
            bare: HashMap::new(),
        };
        let table = RemapTable {
            keymaps: vec![keymap],
        };
        assert!(table.resolve("x.exe", combo("C-h")).is_some());
        // Extra Shift must not trigger the C-h rule (ADR 0004).
        assert!(table.resolve("x.exe", combo("C-S-h")).is_none());
        assert!(table.resolve("x.exe", combo("h")).is_none());
    }

    #[test]
    fn bare_rules_ignore_modifier_state() {
        let keymap = Keymap {
            name: "t".to_string(),
            apps: AppFilter::All,
            exact: HashMap::new(),
            bare: HashMap::from([(combo("CapsLock").vk, combo("LCtrl"))]),
        };
        let table = RemapTable {
            keymaps: vec![keymap],
        };
        let action = table.resolve("x.exe", combo("C-CapsLock")).unwrap();
        assert_eq!(action.kind, RemapKind::KeyOnly);
        assert_eq!(action.target.vk, combo("LCtrl").vk);
    }
}
