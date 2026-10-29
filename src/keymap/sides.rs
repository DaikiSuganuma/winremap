//! Side-modifier arithmetic: which of the eight L/R modifier keys are down,
//! how that collapses to the class-level [`Mods`] the rules match on, and
//! the smallest set of releases and presses that turns one state into the
//! other (ADR 0005, ADR 0017).
//!
//! Pure, so the sender's modifier surgery can be tested here without a
//! keyboard — and so the hook's decision logic can reason about modifiers
//! without reaching into the sender (v1.1 plan B4). Virtual-key codes are
//! plain `u16`s, as everywhere in this crate.

use super::Mods;

/// Physical/logical side-modifier state as a bitset, one bit per L/R key.
/// Kept separate from [`Mods`] because lifting must release the exact keys
/// that are down (LCtrl vs RCtrl), which class-level `Mods` cannot express.
pub type SideMods = u8;

/// Bits in [`SideMods`] covering Alt and Win keys — the modifiers whose lone
/// press-and-release activates the menu bar / Start menu.
pub const ALT_WIN_SIDES: SideMods = 0b1111_0000;

pub const SIDE_VKS: [u16; 8] = [
    0xA0, // LShift
    0xA1, // RShift
    0xA2, // LCtrl
    0xA3, // RCtrl
    0xA4, // LAlt
    0xA5, // RAlt
    0x5B, // LWin
    0x5C, // RWin
];

/// Bit in [`SideMods`] for a side-specific modifier VK, or `None` for
/// non-modifier keys.
pub fn side_bit(vk: u16) -> Option<SideMods> {
    SIDE_VKS.iter().position(|&s| s == vk).map(|i| 1 << i)
}

/// Collapses side-level state to the class-level [`Mods`] used for matching.
pub fn side_mods_to_mods(sides: SideMods) -> Mods {
    let mut mods = Mods::NONE;
    if sides & 0b0000_0011 != 0 {
        mods = mods.with(Mods::SHIFT);
    }
    if sides & 0b0000_1100 != 0 {
        mods = mods.with(Mods::CTRL);
    }
    if sides & 0b0011_0000 != 0 {
        mods = mods.with(Mods::ALT);
    }
    if sides & 0b1100_0000 != 0 {
        mods = mods.with(Mods::WIN);
    }
    mods
}

/// Left-side key per modifier class, used when a target chord needs a
/// modifier that is not physically held.
pub fn class_left_vks(mods: Mods) -> impl Iterator<Item = u16> {
    [
        (Mods::SHIFT, 0xA0u16),
        (Mods::CTRL, 0xA2),
        (Mods::ALT, 0xA4),
        (Mods::WIN, 0x5B),
    ]
    .into_iter()
    .filter(move |(class, _)| mods.contains(*class))
    .map(|(_, vk)| vk)
}

/// Side bits whose modifier class is NOT wanted by `target_mods`.
pub fn sides_to_lift(held: SideMods, target_mods: Mods) -> SideMods {
    let mut lift = 0;
    for i in 0..SIDE_VKS.len() {
        let bit = 1 << i;
        if held & bit != 0 && !target_mods.contains(side_mods_to_mods(bit)) {
            lift |= bit;
        }
    }
    lift
}

/// Keys whose hardware scan code carries the extended-key prefix; without
/// this flag apps would see e.g. numpad-Del instead of navigation Del.
pub fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        0x21..=0x28 // PgUp/PgDn/End/Home/arrows
        | 0x2D | 0x2E // Insert/Delete
        | 0x5B | 0x5C | 0x5D // LWin/RWin/Apps
        | 0xA3 | 0xA5 // RCtrl/RAlt
    )
}

/// Left-side key and its [`SideMods`] bit per modifier class, used when a
/// transition must synthesize a modifier that is not physically held.
const CLASS_LEFT: [(Mods, u16, SideMods); 4] = [
    (Mods::SHIFT, 0xA0, 1),
    (Mods::CTRL, 0xA2, 1 << 2),
    (Mods::ALT, 0xA4, 1 << 4),
    (Mods::WIN, 0x5B, 1 << 6),
];

/// Minimal modifier change to make `current` satisfy `target`, as
/// (releases, presses) side bitmasks. Kept pure for unit testing.
pub fn plan_transition(current: SideMods, target: Mods) -> (SideMods, SideMods) {
    let release = sides_to_lift(current, target);
    let have = side_mods_to_mods(current & !release);
    let mut press = 0;
    for (class, _, bit) in CLASS_LEFT {
        if target.contains(class) && !have.contains(class) {
            press |= bit;
        }
    }
    (release, press)
}
