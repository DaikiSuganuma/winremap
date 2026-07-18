//! `SendInput` wrapper: emits substitute keys and temporarily lifts held
//! modifiers so exact-rule targets arrive with exactly the configured
//! modifier state (config-spec §3.1, ADR 0005).
//!
//! Called from inside the keyboard hook callback, so everything here works on
//! fixed-size stack arrays — no allocation, no locking (AGENTS.md invariant 2).

use std::sync::OnceLock;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, MAP_VIRTUAL_KEY_TYPE, MapVirtualKeyW, SendInput, VIRTUAL_KEY,
};

use winremap::keymap::{KeyCombo, MAX_MACRO_LEN, Mods};

/// `dwExtraInfo` marker on events we inject as remap output (the substitute
/// key, or a modifier acting as one). The hook feeds these back into its
/// logical modifier tracking so remapped modifiers still form chords.
pub const MARKER_REMAP: usize = 0x57524D00; // "WRM\0"
/// Marker for transient lift/restore compensation events. These must NOT be
/// tracked: they change what applications see, not which chord is logically
/// held (ADR 0006).
pub const MARKER_COMPENSATION: usize = 0x57524D01;

/// Physical/logical side-modifier state as a bitset, one bit per L/R key.
/// Kept separate from [`Mods`] because lifting must release the exact keys
/// that are down (LCtrl vs RCtrl), which class-level `Mods` cannot express.
pub type SideMods = u8;

/// Bits in [`SideMods`] covering Alt and Win keys — the modifiers whose lone
/// press-and-release activates the menu bar / Start menu.
pub const ALT_WIN_SIDES: SideMods = 0b1111_0000;

/// Reserved VK used as a "menu mask": tapping it between an Alt/Win down and
/// up breaks the lone-tap pattern Windows uses to trigger the menu bar and
/// Start menu, while applications ignore the key itself (ADR 0015; same
/// technique as Keyhac/AutoHotkey).
const VK_MENU_MASK: u16 = 0xFF;

const SIDE_VKS: [u16; 8] = [
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
fn class_left_vks(mods: Mods) -> impl Iterator<Item = u16> {
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
fn sides_to_lift(held: SideMods, target_mods: Mods) -> SideMods {
    let mut lift = 0;
    for i in 0..SIDE_VKS.len() {
        let bit = 1 << i;
        if held & bit != 0 && !target_mods.contains(side_mods_to_mods(bit)) {
            lift |= bit;
        }
    }
    lift
}

/// Precomputed VK→scan-code table. Scan codes matter because terminal apps
/// and games read them instead of the VK; computing them per keystroke would
/// put a Win32 call inside the hook.
static SCAN_CODES: OnceLock<[u16; 256]> = OnceLock::new();

/// Must be called once at startup, before the keyboard hook is installed.
pub fn init_scan_codes() {
    SCAN_CODES.get_or_init(|| {
        let mut table = [0u16; 256];
        for (vk, entry) in table.iter_mut().enumerate() {
            // MAPVK_VK_TO_VSC == 0. SAFETY: no pointers involved; unknown VKs
            // simply map to 0, which SendInput accepts.
            *entry = unsafe { MapVirtualKeyW(vk as u32, MAP_VIRTUAL_KEY_TYPE(0)) } as u16;
        }
        table
    });
}

fn scan_code(vk: u16) -> u16 {
    // Fallback 0 only happens if init_scan_codes was skipped; events still
    // work for VK-reading apps, so don't panic inside the hook.
    SCAN_CODES.get().map_or(0, |t| t[usize::from(vk) & 0xFF])
}

/// Keys whose hardware scan code carries the extended-key prefix; without
/// this flag apps would see e.g. numpad-Del instead of navigation Del.
fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        0x21..=0x28 // PgUp/PgDn/End/Home/arrows
        | 0x2D | 0x2E // Insert/Delete
        | 0x5B | 0x5C | 0x5D // LWin/RWin/Apps
        | 0xA3 | 0xA5 // RCtrl/RAlt
    )
}

fn key_input(vk: u16, up: bool, extra_info: usize) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if is_extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan_code(vk),
                dwFlags: flags,
                time: 0,
                dwExtraInfo: extra_info,
            },
        },
    }
}

/// Chord batches hold at most: 8 side releases + 4 added presses + 1 key.
const CHORD_BATCH: usize = 16;
/// Macro batches: worst-case per element is a full modifier transition
/// (mask 2 + 8 releases + 4 presses) plus the key tap, and the final restore
/// is another full transition. All one `SendInput` call so no real input
/// interleaves.
const MACRO_BATCH: usize = MAX_MACRO_LEN * 16 + 16;

struct Batch<const N: usize> {
    inputs: [INPUT; N],
    len: usize,
}

impl<const N: usize> Batch<N> {
    fn new() -> Self {
        Self {
            inputs: [INPUT::default(); N],
            len: 0,
        }
    }

    fn push(&mut self, input: INPUT) {
        // Silently dropping would corrupt modifier state; the bound is a
        // static property of the sequences built below, so overflow is a
        // programming error caught in tests, not a runtime condition.
        debug_assert!(self.len < N);
        if self.len < N {
            self.inputs[self.len] = input;
            self.len += 1;
        }
    }

    fn send(&self) {
        if self.len == 0 {
            return;
        }
        // SAFETY: the slice points at initialized INPUT structs on our stack.
        let sent = unsafe {
            SendInput(
                &self.inputs[..self.len],
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        // A short send means the input queue was blocked (e.g. secure
        // desktop); nothing safe to do from inside the hook, and retrying
        // could reorder events, so drop it.
        let _ = sent;
    }
}

/// What a remapped press changed besides the key itself, needed to undo the
/// modifier surgery on release.
#[derive(Clone, Copy, Default)]
pub struct ModAdjustment {
    /// Side keys that were physically/logically down and we released.
    pub lifted: SideMods,
    /// Left-side keys we pressed because the target chord needed them.
    pub added: Mods,
}

/// Emits the press of an exact-rule target: lifts unwanted held modifiers,
/// presses missing ones, then the target key (ADR 0005).
pub fn send_exact_down(target: KeyCombo, held: SideMods) -> ModAdjustment {
    let lifted = sides_to_lift(held, target.mods);
    let held_mods = side_mods_to_mods(held);
    let mut batch = Batch::<CHORD_BATCH>::new();

    // The original chord key was suppressed, so without a mask the injected
    // Alt/Win up would read as a lone tap and pop the menu / Start menu.
    if lifted & ALT_WIN_SIDES != 0 {
        push_menu_mask(&mut batch);
    }
    for (i, &vk) in SIDE_VKS.iter().enumerate() {
        if lifted & (1 << i) != 0 {
            batch.push(key_input(vk, true, MARKER_COMPENSATION));
        }
    }
    let mut added = Mods::NONE;
    for vk in class_left_vks(target.mods) {
        let class = side_mods_to_mods(side_bit(vk).unwrap_or(0));
        if !held_mods.contains(class) {
            added = added.with(class);
            batch.push(key_input(vk, false, MARKER_COMPENSATION));
        }
    }
    batch.push(key_input(target.vk, false, MARKER_REMAP));
    batch.send();
    ModAdjustment { lifted, added }
}

/// Auto-repeat of an already-pressed exact-rule target: modifiers were
/// already adjusted at the initial press, so only the key repeats.
pub fn send_exact_repeat(target_vk: u16) {
    let mut batch = Batch::<CHORD_BATCH>::new();
    batch.push(key_input(target_vk, false, MARKER_REMAP));
    batch.send();
}

/// Emits the release of an exact-rule target and restores the modifier state
/// to match what is still physically held (`still_held`, which the user may
/// have changed while the remapped key was down).
pub fn send_exact_up(target: KeyCombo, adjustment: ModAdjustment, still_held: SideMods) {
    let mut batch = Batch::<CHORD_BATCH>::new();
    batch.push(key_input(target.vk, true, MARKER_REMAP));
    for vk in class_left_vks(adjustment.added) {
        batch.push(key_input(vk, true, MARKER_COMPENSATION));
    }
    for (i, &vk) in SIDE_VKS.iter().enumerate() {
        // Only restore keys the user is still holding; re-pressing a
        // modifier they already released would wedge it down.
        if adjustment.lifted & (1 << i) != 0 && still_held & (1 << i) != 0 {
            batch.push(key_input(vk, false, MARKER_COMPENSATION));
        }
    }
    batch.send();
}

fn push_menu_mask<const N: usize>(batch: &mut Batch<N>) {
    batch.push(key_input(VK_MENU_MASK, false, MARKER_COMPENSATION));
    batch.push(key_input(VK_MENU_MASK, true, MARKER_COMPENSATION));
}

/// Replaces a physical Alt/Win key-up whose chord we consumed: the mask tap
/// must land *before* the up event, so the hook suppresses the physical
/// release and this emits `[mask, up]` as one ordered batch. The up carries
/// `MARKER_REMAP` so the hook's modifier tracking follows it (ADR 0015).
pub fn send_masked_modifier_up(vk: u16) {
    let mut batch = Batch::<CHORD_BATCH>::new();
    push_menu_mask(&mut batch);
    batch.push(key_input(vk, true, MARKER_REMAP));
    batch.send();
}

/// Bare-key rule output: substitute the key only, leave modifiers alone
/// (config-spec §3.2).
pub fn send_key_only(target_vk: u16, up: bool) {
    let mut batch = Batch::<CHORD_BATCH>::new();
    batch.push(key_input(target_vk, up, MARKER_REMAP));
    batch.send();
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
fn plan_transition(current: SideMods, target: Mods) -> (SideMods, SideMods) {
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

/// Emits the planned transition and returns the new side state. Releases
/// come first (masked when they involve Alt/Win) so a chord never gains
/// extra modifiers mid-flight.
fn transition_mods<const N: usize>(
    batch: &mut Batch<N>,
    current: SideMods,
    release: SideMods,
    press: SideMods,
) -> SideMods {
    if release & ALT_WIN_SIDES != 0 {
        push_menu_mask(batch);
    }
    for (i, &vk) in SIDE_VKS.iter().enumerate() {
        if release & (1 << i) != 0 {
            batch.push(key_input(vk, true, MARKER_COMPENSATION));
        }
    }
    for (i, &vk) in SIDE_VKS.iter().enumerate() {
        if press & (1 << i) != 0 {
            batch.push(key_input(vk, false, MARKER_COMPENSATION));
        }
    }
    (current & !release) | press
}

/// Macro output (ADR 0012): taps each chord once, in one `SendInput` batch
/// so real keystrokes cannot interleave. Modifiers move by *diff* between
/// elements instead of a full lift/re-press per element (ADR 0017): a macro
/// like C-Right → C-Left → C-S-Right with Ctrl physically held never touches
/// Ctrl at all, which keeps apps that sample modifier state asynchronously
/// from misreading the chord. Runs entirely within one hook callback, so
/// `held` cannot change midway.
pub fn send_sequence(sequence: &[KeyCombo], held: SideMods) {
    let mut batch = Batch::<MACRO_BATCH>::new();
    let mut current = held;
    for combo in sequence.iter().take(MAX_MACRO_LEN) {
        let (release, press) = plan_transition(current, combo.mods);
        current = transition_mods(&mut batch, current, release, press);
        batch.push(key_input(combo.vk, false, MARKER_REMAP));
        batch.push(key_input(combo.vk, true, MARKER_REMAP));
    }
    // Restore the physically held state: drop synthesized sides, re-press
    // held ones we released along the way.
    let release = current & !held;
    let press = held & !current;
    transition_mods(&mut batch, current, release, press);
    batch.send();
}

#[cfg(test)]
mod tests {
    use super::*;

    const LSHIFT: SideMods = 1;
    const LCTRL: SideMods = 1 << 2;
    const RCTRL: SideMods = 1 << 3;
    const LALT: SideMods = 1 << 4;

    #[test]
    fn transition_keeps_already_satisfied_modifiers() {
        // The C-t macro case: Ctrl physically held, element needs Ctrl —
        // nothing to release or press (ADR 0017).
        assert_eq!(plan_transition(LCTRL, Mods::CTRL), (0, 0));
        // Either physical side satisfies the class.
        assert_eq!(plan_transition(RCTRL, Mods::CTRL), (0, 0));
    }

    #[test]
    fn transition_adds_only_the_missing_class() {
        // C-Right → C-S-Right: keep Ctrl, add Shift.
        assert_eq!(
            plan_transition(LCTRL, Mods::CTRL.with(Mods::SHIFT)),
            (0, LSHIFT)
        );
    }

    #[test]
    fn transition_swaps_unrelated_modifiers() {
        // A-a's elements: Alt held but the element needs Ctrl only.
        assert_eq!(plan_transition(LALT, Mods::CTRL), (LALT, LCTRL));
    }

    #[test]
    fn transition_releases_everything_for_bare_targets() {
        assert_eq!(
            plan_transition(LCTRL | LSHIFT, Mods::NONE),
            (LCTRL | LSHIFT, 0)
        );
        assert_eq!(plan_transition(0, Mods::NONE), (0, 0));
    }
}
