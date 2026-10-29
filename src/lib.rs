//! winremap core: key notation parsing, keymap resolution, configuration
//! loading and editing, and the pure halves of two features (the macro
//! recorder's state machine, the IME indicator's settings).
//!
//! Everything here is OS-independent pure logic, so it runs under `cargo
//! test` on headless CI and never names the `windows` crate — a virtual-key
//! code is a `u16` (project brief §9; `tests/invariants.rs` checks it). The
//! Win32 layers — the hook, `SendInput`, the foreground watch, the tray, the
//! GUI and the feature threads — live in the binary (`src/main.rs` and the
//! modules it declares) and depend on this crate, never the other way round.

pub mod config;
pub mod ime_indicator_settings;
pub mod keymap;
pub mod recorder;
