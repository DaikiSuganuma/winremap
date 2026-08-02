//! Key notation parsing and remap resolution.
//!
//! This module is pure logic with no OS dependency so it runs under `cargo
//! test` on headless CI (project brief §9). Virtual-key codes are plain `u16`
//! values matching the Win32 `VK_*` constants, hardcoded here to avoid a
//! `windows` crate dependency in the testable core.
//!
//! Submodules: `parse` (notation → combos), `table` (compiled keymaps and
//! resolution), `control` (the control code a combination carries). Shared
//! primitive types live here.

mod control;
mod layout;
mod parse;
mod table;
// Visible crate-wide: the reference US and JP keyboards it builds are what
// let the config tests exercise both layouts too (ADR 0063).
#[cfg(test)]
pub(crate) mod tests;

pub use control::{ControlCode, control_code};
pub use layout::{Layout, OEM_ALIASES, OEM_VKS, oem_alias};
pub use parse::{
    InputPattern, KeyParseError, SPECIAL_KEY_NAMES, is_modifier_vk, key_name, key_name_to_vk,
    parse_input_pattern, parse_key_combo, suggest_key_name, vk_config_name, vk_display_name,
};
pub use table::{AppFilter, Keymap, Output, RemapTable, Resolution};

/// Upper bound for macro outputs and thus for the sender's input batch size
/// (ADR 0012). Raising this requires revisiting the stack budget in sender.rs.
pub const MAX_MACRO_LEN: usize = 8;

/// Upper bound for the per-stroke macro pacing delay (ADR 0018): even an
/// 8-stroke macro must stay far below the low-level-hook timeout.
pub const MAX_MACRO_DELAY_MS: u32 = 15;

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

/// Renders in config notation (`C-S-h`) so debug output matches what the
/// user would write in config.toml.
///
/// A symbol key comes out as its layout-independent alias (`C-Oem1`), since
/// `Display` has no keyboard to ask. That spelling is always accepted by the
/// parser, so the output is never something the config would reject — but
/// where the keyboard *is* known, [`combo_notation`] says `C-;` instead, and
/// that is what the user interface shows.
impl std::fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&modifier_prefixes(self.mods))?;
        f.write_str(&parse::vk_display_name(self.vk))
    }
}

/// A chord as this keyboard's owner would write it: `C-;` rather than
/// `C-Oem1`.
///
/// Everything the settings window shows and matches on goes through here, so
/// the rules table, the comment lookup and the file all spell a rule the same
/// way (ADR 0063).
#[must_use]
pub fn combo_notation(combo: &KeyCombo, layout: &Layout) -> String {
    format!(
        "{}{}",
        modifier_prefixes(combo.mods),
        parse::key_name(combo.vk, layout).unwrap_or_else(|| parse::vk_display_name(combo.vk))
    )
}

fn modifier_prefixes(mods: Mods) -> String {
    let mut text = String::new();
    for (flag, prefix) in [
        (Mods::CTRL, "C-"),
        (Mods::ALT, "A-"),
        (Mods::SHIFT, "S-"),
        (Mods::WIN, "W-"),
    ] {
        if mods.contains(flag) {
            text.push_str(prefix);
        }
    }
    text
}
