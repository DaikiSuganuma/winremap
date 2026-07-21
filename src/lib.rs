//! winremap core: key notation parsing, keymap resolution, and configuration
//! loading. Everything here is OS-independent pure logic so it runs on
//! headless CI; the Win32 layers (hook/sender/window) live in the binary and
//! arrive in Phase 2.

pub mod config;
pub mod ime_indicator_settings;
pub mod keymap;
pub mod recorder;
