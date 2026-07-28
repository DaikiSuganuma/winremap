//! Which key a virtual-key code means, in words, for the log (ADR 0058).
//!
//! `vk_display_name` in the keymap crate answers the *config's* question —
//! "what do I write in the file for this key" — and has nothing to say about
//! the keys the notation cannot name yet: OEM punctuation, the numpad, the
//! IME keys. Those come out as `0xBF` or `0x61`, which tells a reader nothing
//! about the key they just pressed (owner request 2026-07-29).
//!
//! So this module answers the *log's* question instead, in three steps:
//!
//! 1. the config name, when the key has one — `a`, `Down`, `LCtrl`;
//! 2. a fixed name, for keys whose meaning does not depend on the keyboard
//!    layout (the numpad, the lock keys, the IME keys, media keys);
//! 3. otherwise the character *this* keyboard produces, which only Windows
//!    knows — `0xC0` is `@` on a JP layout and `` ` `` on a US one, so a table
//!    here would be wrong for half its readers.
//!
//! The raw code stays in parentheses for 2 and 3. It is what a bug report
//! needs, and it is the honest signal that the key has no name to put in the
//! config yet — a bare `/` would read as an invitation to write `"/" = ...`,
//! which the parser rejects (the OEM keys are still deferred, see the TODO in
//! `keymap/parse.rs`).
//!
//! Nothing here runs on the hook's path: the hook queues raw codes and the
//! message loop formats them (ADR 0016).

use windows::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_CHAR, MapVirtualKeyW};

use winremap::keymap::{KeyCombo, Mods, vk_config_name};

/// One key, as a reader of the log would name it.
pub fn key(vk: u16) -> String {
    if let Some(name) = vk_config_name(vk) {
        return name;
    }
    if let Some(name) = fixed_name(vk) {
        return format!("{name} (0x{vk:02X})");
    }
    match layout_char(vk) {
        Some(character) => format!("{character} (0x{vk:02X})"),
        // Nothing to say beyond the code itself — the key produces no
        // character and is not one of the named ones.
        None => format!("0x{vk:02X}"),
    }
}

/// A chord in the config's own notation (`C-S-h`), with [`key`] naming the
/// key itself. The prefixes match `KeyCombo`'s `Display` on purpose: the log
/// and the config file should spell a chord the same way.
pub fn combo(combo: &KeyCombo) -> String {
    let mut text = String::new();
    for (flag, prefix) in [
        (Mods::CTRL, "C-"),
        (Mods::ALT, "A-"),
        (Mods::SHIFT, "S-"),
        (Mods::WIN, "W-"),
    ] {
        if combo.mods.contains(flag) {
            text.push_str(prefix);
        }
    }
    text.push_str(&key(combo.vk));
    text
}

/// Keys with no config name whose identity is the same on every keyboard, so
/// naming them here cannot be wrong. Layout-dependent keys are deliberately
/// absent — [`layout_char`] asks Windows about those instead.
fn fixed_name(vk: u16) -> Option<&'static str> {
    Some(match vk {
        0x03 => "Cancel",
        0x0C => "Clear",
        // The sideless modifiers. A physical press always arrives as the
        // L/R form; these turn up in other software's injected events.
        0x10 => "Shift",
        0x11 => "Ctrl",
        0x12 => "Alt",
        0x13 => "Pause",
        // IME keys. A JP keyboard has three of its own, and they are the
        // ones most likely to be pressed by accident and wondered about.
        0x15 => "Kana",
        0x19 => "Kanji",
        0x1C => "Convert",    // 変換
        0x1D => "NonConvert", // 無変換
        0x1E => "Accept",
        0x1F => "ModeChange",
        0x29 => "Select",
        0x2A => "Print",
        0x2B => "Execute",
        0x2C => "PrintScreen",
        0x2F => "Help",
        0x5F => "Sleep",
        0x60..=0x69 => return NUMPAD_DIGITS.get(usize::from(vk - 0x60)).copied(),
        0x6A => "NumMultiply",
        0x6B => "NumAdd",
        0x6C => "NumSeparator",
        0x6D => "NumSubtract",
        0x6E => "NumDecimal",
        0x6F => "NumDivide",
        0x90 => "NumLock",
        0x91 => "ScrollLock",
        0xA6 => "BrowserBack",
        0xA7 => "BrowserForward",
        0xA8 => "BrowserRefresh",
        0xA9 => "BrowserStop",
        0xAA => "BrowserSearch",
        0xAB => "BrowserFavorites",
        0xAC => "BrowserHome",
        0xAD => "VolumeMute",
        0xAE => "VolumeDown",
        0xAF => "VolumeUp",
        0xB0 => "MediaNext",
        0xB1 => "MediaPrev",
        0xB2 => "MediaStop",
        0xB3 => "MediaPlayPause",
        0xB4 => "LaunchMail",
        0xB5 => "LaunchMedia",
        0xB6 => "LaunchApp1",
        0xB7 => "LaunchApp2",
        // "OEM specific" in the Win32 headers, but on a JP 106 keyboard this
        // is where 半角/全角 arrives, which is exactly the key a Japanese
        // user is most likely to be looking for in the log.
        0xF3 | 0xF4 => "Hankaku/Zenkaku",
        _ => return None,
    })
}

/// Spelled out rather than computed: `format!("Num{n}")` would allocate on
/// every log line for the one case that does not need to.
const NUMPAD_DIGITS: [&str; 10] = [
    "Num0", "Num1", "Num2", "Num3", "Num4", "Num5", "Num6", "Num7", "Num8", "Num9",
];

/// The unshifted character this keyboard's layout puts on the key, if any.
fn layout_char(vk: u16) -> Option<char> {
    // SAFETY: no pointers and no handles — MapVirtualKeyW reads the calling
    // thread's keyboard layout and returns a plain integer. Zero means "this
    // key produces no character", which is handled below.
    let mapped = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_CHAR) };
    // The character is the low word; the high bit only marks a dead key (an
    // accent on a European layout), and such a key still has a face worth
    // printing.
    let character = char::from_u32(mapped & 0xFFFF)?;
    (!character.is_control() && !character.is_whitespace()).then_some(character)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key the config can name is named the way the config names it —
    /// otherwise the log would teach a spelling the file rejects.
    #[test]
    fn a_config_name_wins() {
        assert_eq!(key(0x41), "a");
        assert_eq!(key(0x28), "Down");
        assert_eq!(key(0xA2), "LCtrl");
        assert_eq!(key(0x71), "F2");
    }

    /// The two cases from the owner's report. Neither has a config name, so
    /// both keep their code — but a reader can now tell what was pressed.
    #[test]
    fn keys_without_a_config_name_still_say_what_they_are() {
        assert_eq!(key(0x61), "Num1 (0x61)");
        // 0xBF is VK_OEM_2, `/` on both the US and the JP layout — the only
        // layout-dependent step, so this asserts the shape and not the face.
        let oem = key(0xBF);
        assert!(
            oem.ends_with(" (0xBF)") || oem == "0xBF",
            "unexpected rendering: {oem}"
        );
    }

    #[test]
    fn a_chord_reads_as_the_config_spells_it() {
        let combo = winremap::keymap::parse_key_combo("C-S-h").expect("parses");
        assert_eq!(super::combo(&combo), "C-S-h");
    }
}
