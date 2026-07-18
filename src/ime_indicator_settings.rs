//! Settings for the IME status indicator (docs/08_ime-indicator-design.md).
//!
//! Only the pure config side of the feature lives in the library so it is
//! testable on headless CI; the Win32 side (detection thread, overlay
//! window) is a separate module tree in the binary, `src/ime_indicator/`
//! (ADR 0020, plan Phase I3). Carried on `RemapTable` so a tray reload
//! swaps indicator settings together with the rules (design doc §3.4).

/// Display duration bounds, in ms (design doc §3.4).
pub const MIN_INDICATOR_DURATION_MS: u32 = 100;
pub const MAX_INDICATOR_DURATION_MS: u32 = 5000;

/// Panel edge bounds, in logical pixels (design doc §3.4).
pub const MIN_INDICATOR_SIZE: u32 = 32;
pub const MAX_INDICATOR_SIZE: u32 = 256;

/// Compiled `[ime_indicator]` section; defaults apply when the section or a
/// field is omitted. The feature is opt-in, so `enabled` defaults to false.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct IndicatorSettings {
    pub enabled: bool,
    /// How long the panel stays visible before fading out, in ms.
    pub duration_ms: u32,
    /// Panel edge length in logical pixels.
    pub size: u32,
    /// Overall panel alpha (0 = invisible, 255 = opaque).
    pub opacity: u8,
}

impl Default for IndicatorSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            duration_ms: 800,
            size: 96,
            opacity: 200,
        }
    }
}
