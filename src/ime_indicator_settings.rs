//! Settings for the IME status indicator (docs/v0.1/05_ime-indicator-design.md).
//!
//! Only the pure config side of the feature lives in the library so it is
//! testable on headless CI; the Win32 side (detection thread, overlay
//! window) is a separate module tree in the binary, `src/ime_indicator/`
//! (ADR 0020, plan Phase I3). Carried on `RemapTable` so a tray reload
//! swaps indicator settings together with the rules (design doc §3.4).

use crate::keymap::KeyCombo;

/// Display duration bounds, in ms (design doc §3.4).
pub const MIN_INDICATOR_DURATION_MS: u32 = 100;
pub const MAX_INDICATOR_DURATION_MS: u32 = 5000;

/// Panel edge bounds, in logical pixels (design doc §3.4).
pub const MIN_INDICATOR_SIZE: u32 = 32;
pub const MAX_INDICATOR_SIZE: u32 = 256;

/// Compiled `[ime_indicator]` section; defaults apply when the section or a
/// field is omitted. The feature is opt-in, so `enabled` defaults to false.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct IndicatorSettings {
    pub enabled: bool,
    /// How long the panel stays visible before fading out, in ms.
    pub duration_ms: u32,
    /// Panel edge length in logical pixels.
    pub size: u32,
    /// Overall panel alpha (0 = invisible, 255 = opaque).
    pub opacity: u8,
    /// Extra IME toggle chords on top of the built-in VK candidates, for
    /// user-assigned toggles like Ctrl+Space (Windows 11 IME option).
    /// Matched on the full chord — modifiers included (ADR 0021).
    pub trigger_keys: Vec<KeyCombo>,
    /// Show the target application's exe name under the glyph (ADR 0024).
    pub show_app_name: bool,
    /// Tint the mouse cursor while the IME is on (ADR 0067). Off by default:
    /// this one replaces cursors for the whole session, not just inside
    /// WinRemap, so it is not something to switch on for somebody.
    pub change_cursor_color: bool,
    /// The tint as R/G/B. Applied to the cursor's own shape rather than
    /// replacing it, so the black outline stays black and the white body
    /// takes this colour.
    pub cursor_color: (u8, u8, u8),
}

impl Default for IndicatorSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            duration_ms: 800,
            size: 96,
            opacity: 200,
            trigger_keys: Vec::new(),
            show_app_name: false,
            change_cursor_color: false,
            // A warm orange: distinct from the black-and-white the cursors
            // start as, and readable on both light and dark backgrounds.
            cursor_color: (0xE0, 0x5A, 0x2B),
        }
    }
}

/// Parses `#rrggbb` (or `rrggbb`) into R/G/B.
///
/// Deliberately the only accepted spelling: colour names would need a table
/// that is either short enough to disappoint or long enough to argue about,
/// and this value is written once.
pub fn parse_hex_color(text: &str) -> Option<(u8, u8, u8)> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    Some((byte(0)?, byte(2)?, byte(4)?))
}
