//! What this keyboard prints on its symbol keys (ADR 0063).
//!
//! The virtual-key codes Win32 calls "OEM" mean different keys on different
//! keyboards: `0xC0` is `@` on a Japanese 106 keyboard and `` ` `` on a US
//! one. A table here would therefore be wrong for half its readers, which is
//! why the symbol keys went unsupported for six versions.
//!
//! The answer is to hold a *snapshot* of what Windows says about the keyboard
//! that is actually attached, and to look rules up against that. This module
//! is only the snapshot — plain data, no OS calls, so both layouts can be
//! tested in one process on headless CI (project brief §9). The binary's
//! `layout::refresh` fills one in from `MapVirtualKeyW` / `VkKeyScanW`.
//!
//! Two directions are needed and they are not inverses of each other:
//!
//! - **face**: which character a key prints, for naming a key in the log and
//!   in the settings window;
//! - **shifted**: which key a character sits on when it needs Shift, used for
//!   nothing but the error that tells the user to write `S-2` instead of `@`.

/// The virtual-key codes whose meaning depends on the keyboard.
///
/// From Win32's Virtual-Key Codes: `VK_OEM_1`, `VK_OEM_PLUS`, `VK_OEM_COMMA`,
/// `VK_OEM_MINUS`, `VK_OEM_PERIOD`, `VK_OEM_2` … `VK_OEM_8`, `VK_OEM_102`.
/// `VK_OEM_AUTO` / `VK_OEM_ENLW` (0xF3/0xF4) are deliberately absent: on a JP
/// keyboard they are 半角/全角, an IME key rather than a symbol, and the log
/// already names them (`keyname.rs`).
pub const OEM_VKS: [u16; 13] = [
    0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0, 0xDB, 0xDC, 0xDD, 0xDE, 0xDF, 0xE2,
];

/// A layout-independent name for every key in [`OEM_VKS`], spelled the way
/// Win32 spells it.
///
/// These are the aliases a config can use to name a symbol key without
/// depending on the keyboard. Win32's own names are used rather than invented
/// ones (`Semicolon`, `Slash`) because **an invented name would be a lie on
/// some keyboard**: the key called `VK_OEM_1` prints `;` on a US layout and
/// `:` on a JP one. `Oem1` claims nothing, so it cannot be wrong.
pub const OEM_ALIASES: [(u16, &str); 13] = [
    (0xBA, "Oem1"),
    (0xBB, "OemPlus"),
    (0xBC, "OemComma"),
    (0xBD, "OemMinus"),
    (0xBE, "OemPeriod"),
    (0xBF, "Oem2"),
    (0xC0, "Oem3"),
    (0xDB, "Oem4"),
    (0xDC, "Oem5"),
    (0xDD, "Oem6"),
    (0xDE, "Oem7"),
    (0xDF, "Oem8"),
    (0xE2, "Oem102"),
];

/// A snapshot of one keyboard's symbol keys.
///
/// [`Layout::empty`] knows no symbol keys at all: every symbol character is
/// then an unknown key name. That is the honest answer when the layout could
/// not be read, and it is what keeps the library usable without Windows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    /// `(vk, character)` for the keys that print something unshifted, sorted
    /// by vk. Sorted so that a character sitting on two keys — `\` is on both
    /// `VK_OEM_5` and `VK_OEM_102` on a US layout — always resolves to the
    /// same one of them rather than to whichever happened to be listed first.
    faces: Vec<(u16, char)>,
    /// `(character, vk)` for every character this keyboard reaches only with
    /// Shift held — including the ones on the number row, since `@` is
    /// `Shift`+`2` on a US layout and saying exactly that is the point.
    ///
    /// This doubles as the shifted-face table, read the other way round: one
    /// key has one shifted character, so the two are the same fact and are
    /// not stored twice.
    needs_shift: Vec<(char, u16)>,
}

impl Layout {
    /// A layout that knows no symbol keys.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Builds a snapshot. Input order does not matter; entries are sorted and
    /// de-duplicated so lookups are deterministic.
    #[must_use]
    pub fn new(faces: Vec<(u16, char)>, needs_shift: Vec<(char, u16)>) -> Self {
        let mut layout = Self { faces, needs_shift };
        layout.faces.sort_unstable();
        layout.faces.dedup();
        layout.needs_shift.sort_unstable();
        layout.needs_shift.dedup();
        layout
    }

    /// The key that prints `character` without Shift, if this keyboard has
    /// one. The lowest virtual-key code wins when two keys print the same
    /// character (see `faces`).
    #[must_use]
    pub fn key_printing(&self, character: char) -> Option<u16> {
        self.faces
            .iter()
            .find(|(_, face)| *face == character)
            .map(|(vk, _)| *vk)
    }

    /// The key that prints `character` **with Shift held**, if any. Used to
    /// turn "you cannot write `@`" into "write `S-2`".
    #[must_use]
    pub fn shifted_key_printing(&self, character: char) -> Option<u16> {
        self.needs_shift
            .iter()
            .find(|(shifted, _)| *shifted == character)
            .map(|(_, vk)| *vk)
    }

    /// What this key prints unshifted.
    #[must_use]
    pub fn face(&self, vk: u16) -> Option<char> {
        self.faces
            .iter()
            .find(|(code, _)| *code == vk)
            .map(|(_, face)| *face)
    }

    /// What this key prints with Shift held.
    #[must_use]
    pub fn shifted_face(&self, vk: u16) -> Option<char> {
        self.needs_shift
            .iter()
            .find(|(_, code)| *code == vk)
            .map(|(face, _)| *face)
    }

    /// Every symbol key this keyboard has, as `(character, alias)` pairs in
    /// virtual-key order — what the settings window shows so the reader can
    /// see the names **their own** keyboard answers to.
    #[must_use]
    pub fn symbol_keys(&self) -> Vec<(char, &'static str)> {
        self.faces
            .iter()
            .filter_map(|(vk, face)| oem_alias(*vk).map(|alias| (*face, alias)))
            .collect()
    }

    /// Whether the snapshot holds anything at all. False means symbol keys
    /// cannot be written by their character, only by their alias.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }
}

/// The layout-independent alias for an OEM virtual-key code.
#[must_use]
pub fn oem_alias(vk: u16) -> Option<&'static str> {
    OEM_ALIASES
        .iter()
        .find(|(code, _)| *code == vk)
        .map(|(_, name)| *name)
}

/// The virtual-key code an alias names, case-insensitively.
#[must_use]
pub fn vk_for_alias(name: &str) -> Option<u16> {
    OEM_ALIASES
        .iter()
        .find(|(_, alias)| alias.eq_ignore_ascii_case(name))
        .map(|(vk, _)| *vk)
}
