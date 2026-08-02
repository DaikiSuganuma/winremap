//! What the attached keyboard prints on its symbol keys, asked of Windows
//! (ADR 0063).
//!
//! The whole of the layout question is two Win32 calls, and this module is
//! the only place either of them is made:
//!
//! - `MapVirtualKeyW(vk, MAPVK_VK_TO_CHAR)` — which character a key prints;
//! - `VkKeyScanW(character)` — which key a character is on, and whether Shift
//!   is needed to reach it.
//!
//! Both read the **calling thread's** keyboard layout, which is why the
//! snapshot is taken on the main thread at startup and again on every reload
//! rather than being consulted where it is used. Nothing here runs on the
//! hook's path: rules are resolved to virtual-key codes once, when the config
//! is compiled (invariant 2).
//!
//! The result is plain data (`keymap::Layout`), so every consumer of it —
//! parser, log, settings window — stays testable against a keyboard that is
//! not attached.

use std::sync::{Arc, LazyLock};

use arc_swap::ArcSwap;
use windows::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_CHAR, MapVirtualKeyW, VkKeyScanW};

use winremap::keymap::{Layout, OEM_VKS};

/// The snapshot in force. Starts empty so a caller that runs before
/// [`refresh`] sees "this keyboard has no symbol keys" rather than a guess.
static CURRENT: LazyLock<ArcSwap<Layout>> =
    LazyLock::new(|| ArcSwap::from_pointee(Layout::empty()));

/// The layout the config was last compiled against.
pub fn current() -> Arc<Layout> {
    CURRENT.load_full()
}

/// Re-reads the keyboard and publishes the result.
///
/// Called at startup and before every config reload, which is what makes
/// swapping keyboards a matter of choosing "Reload config" rather than
/// restarting: the rules are re-resolved against whatever is attached now.
pub fn refresh() -> Arc<Layout> {
    let snapshot = Arc::new(read());
    CURRENT.store(Arc::clone(&snapshot));
    snapshot
}

/// Builds a snapshot from the calling thread's keyboard layout.
fn read() -> Layout {
    let faces = OEM_VKS
        .iter()
        .filter_map(|&vk| face(vk).map(|character| (vk, character)))
        .collect();
    // Only the printable ASCII range is probed. A character outside it can
    // still be *written* in a rule — it will be found among the faces above
    // if the keyboard has it unshifted — but the "you need Shift" hint is
    // limited to the characters people actually reach for in key notation.
    let needs_shift = (0x21u8..=0x7E)
        .filter_map(|byte| {
            let character = char::from(byte);
            shift_key(character).map(|vk| (character, vk))
        })
        .collect();
    Layout::new(faces, needs_shift)
}

/// The character this key prints with no modifier held, if it prints one.
fn face(vk: u16) -> Option<char> {
    // SAFETY: no pointers and no handles — MapVirtualKeyW reads the calling
    // thread's keyboard layout and returns a plain integer. Zero means "this
    // key produces no character", which the checks below reject.
    let mapped = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_CHAR) };
    // The character is the low word. The high bit only marks a dead key (an
    // accent on a European layout), and such a key still has a face worth
    // naming — the same reading `keyname.rs` takes.
    let character = char::from_u32(mapped & 0xFFFF)?;
    (!character.is_control() && !character.is_whitespace()).then_some(character)
}

/// The key that produces `character` when Shift is held, if that is the only
/// way this keyboard produces it.
fn shift_key(character: char) -> Option<u16> {
    let mut buffer = [0u16; 2];
    let encoded = character.encode_utf16(&mut buffer);
    // Outside the BMP there is no single UTF-16 unit to ask about, and no
    // keyboard puts such a character on a key.
    let [unit] = encoded else { return None };
    // SAFETY: VkKeyScanW takes a plain UTF-16 code unit by value and returns
    // a plain integer, reading the calling thread's keyboard layout. -1 means
    // "not on this layout", which is handled below.
    let result = unsafe { VkKeyScanW(*unit) };
    if result == -1 {
        return None;
    }
    // Low byte is the virtual-key code, high byte the modifier state:
    // 1 = Shift, 2 = Ctrl, 4 = Alt. Anything but Shift alone is not something
    // the notation can express, so it is reported as unreachable.
    let state = (result >> 8) & 0xFF;
    if state != 1 {
        return None;
    }
    // Cast is lossless: the low byte was masked off first.
    Some((result & 0xFF) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use winremap::keymap::{KeyParseError, oem_alias, parse_key_combo};

    /// These run against whatever keyboard is attached to the machine running
    /// them, so they assert *properties* rather than particular keys. The
    /// characters themselves are pinned by the reference layouts in the
    /// library's own tests, which is where a US and a JP keyboard are both
    /// exercised (ADR 0063).
    #[test]
    fn the_snapshot_describes_real_keys() {
        let layout = read();
        for &vk in &OEM_VKS {
            if let Some(character) = layout.face(vk) {
                assert!(
                    !character.is_control() && !character.is_whitespace(),
                    "0x{vk:02X} reported an unprintable face"
                );
            }
        }
    }

    /// The round trip that matters: every character this keyboard prints on a
    /// symbol key can be written in a rule, and lands on a key that prints it.
    ///
    /// Not asserted as `vk == vk`, because two keys may print the same
    /// character — a US layout puts `\` on both VK_OEM_5 and VK_OEM_102, and
    /// the notation deliberately resolves such a character to one of them.
    #[test]
    fn every_character_this_keyboard_prints_can_be_written() {
        let layout = read();
        for &vk in &OEM_VKS {
            let Some(character) = layout.face(vk) else {
                continue;
            };
            let combo = parse_key_combo(&format!("C-{character}"), &layout)
                .unwrap_or_else(|e| panic!("`C-{character}` (0x{vk:02X}) should parse: {e}"));
            assert_eq!(
                layout.face(combo.vk),
                Some(character),
                "`C-{character}` resolved to a key that does not print it"
            );
        }
    }

    /// The aliases do not depend on the keyboard, so they hold whatever is
    /// plugged in — which is what makes a config portable between machines.
    #[test]
    fn every_alias_names_its_key_on_this_keyboard() {
        let layout = read();
        for &vk in &OEM_VKS {
            let alias = oem_alias(vk).expect("every OEM key has an alias");
            let combo = parse_key_combo(&format!("C-{alias}"), &layout).expect("alias parses");
            assert_eq!(combo.vk, vk, "{alias} named the wrong key");
        }
    }

    /// A character on a shifted face is refused with the spelling that works,
    /// on this keyboard as on the reference ones. Which characters those are
    /// differs by layout, so the test finds one rather than naming it.
    #[test]
    fn a_shifted_character_is_refused_with_advice() {
        let layout = read();
        let Some(shifted) = ('!'..='~').find(|&c| {
            !c.is_alphanumeric()
                && layout.key_printing(c).is_none()
                && layout.shifted_key_printing(c).is_some()
        }) else {
            // A layout where every character is reachable unshifted would be
            // unusual, but it is not a failure.
            return;
        };
        match parse_key_combo(&format!("C-{shifted}"), &layout) {
            Err(KeyParseError::NeedsShift { character, write }) => {
                assert_eq!(character, shifted);
                assert!(write.starts_with("S-"), "advice should say Shift: {write}");
                // The advice has to be something the parser accepts, or it
                // sends the reader in a circle.
                parse_key_combo(&write, &layout).expect("the suggested spelling must parse");
            }
            other => panic!("`C-{shifted}` should need Shift, got {other:?}"),
        }
    }
}
