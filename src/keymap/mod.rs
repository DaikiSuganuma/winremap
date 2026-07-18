//! Key notation parsing and remap resolution.
//!
//! This module is pure logic with no OS dependency so it runs under `cargo
//! test` on headless CI (project brief §9). Virtual-key codes are plain `u16`
//! values matching the Win32 `VK_*` constants, hardcoded here to avoid a
//! `windows` crate dependency in the testable core.
//!
//! Submodules: `parse` (notation → combos), `table` (compiled keymaps and
//! resolution). Shared primitive types live here.

mod parse;
mod table;
#[cfg(test)]
mod tests;

pub use parse::{
    InputPattern, KeyParseError, is_modifier_vk, key_name_to_vk, parse_input_pattern,
    parse_key_combo,
};
pub use table::{AppFilter, Keymap, Output, RemapTable, Resolution};

/// Upper bound for macro outputs and thus for the sender's input batch size
/// (ADR 0012). Raising this requires revisiting the stack budget in sender.rs.
pub const MAX_MACRO_LEN: usize = 8;

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
