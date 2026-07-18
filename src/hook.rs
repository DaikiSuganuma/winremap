//! Low-level keyboard hook: the only place key events are intercepted.
//!
//! The callback is the latency-critical path (AGENTS.md invariant 2): it only
//! reads thread-local state and the atomically-swapped remap table — no
//! allocation, no locking, no I/O, and no panics (a panic would unwind across
//! the FFI boundary into Windows).
//!
//! State lives in `thread_local`s because a WH_KEYBOARD_LL callback always
//! runs on the thread that installed it, via its message loop.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};

use arc_swap::ArcSwapOption;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, MSG, PostQuitMessage, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};
use windows::core::w;

use crate::sender;
use crate::sender::{ModAdjustment, SideMods};
use winremap::keymap::{KeyCombo, RemapKind, RemapTable};

/// The active remap table. Written by the main/reload thread, read wait-free
/// from the hook (ADR 0003). `None` (before startup finishes) passes all
/// events through.
pub static REMAP_TABLE: ArcSwapOption<RemapTable> = ArcSwapOption::const_empty();

/// Tray toggle. Only gates *new* remaps: keys already remapped and held keep
/// their translation until release so no target key gets stuck down.
static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// A key we suppressed and replaced, remembered until its physical release so
/// the matching key-up is translated consistently even if the user changed
/// modifiers mid-press.
#[derive(Clone, Copy)]
struct ActiveRemap {
    target: KeyCombo,
    kind: RemapKind,
    adjustment: ModAdjustment,
}

thread_local! {
    /// Logical side-modifier state: physical keys plus modifiers we injected
    /// as remap output (marker `MARKER_REMAP`), so CapsLock→LCtrl can still
    /// form Ctrl chords. Compensation events are deliberately not tracked.
    static SIDES: Cell<SideMods> = const { Cell::new(0) };
    /// Indexed by original VK. A flat array keeps lookup O(1) without hashing
    /// or allocation inside the callback.
    static ACTIVE: RefCell<[Option<ActiveRemap>; 256]> = const { RefCell::new([None; 256]) };
}

pub fn install() -> windows::core::Result<HHOOK> {
    // SAFETY: the callback is a static fn valid for the process lifetime;
    // passing no module handle is allowed for low-level hooks because they
    // are not injected into other processes.
    unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) }
}

pub fn uninstall(hook: HHOOK) {
    // SAFETY: called once at shutdown with the handle install() returned.
    let _ = unsafe { UnhookWindowsHookEx(hook) };
}

/// Owns the named mutex that prevents a second winremap instance — two
/// processes would install two low-level hooks with undefined interleaving
/// (brief §9-3). Lives in hook.rs because it protects hook integrity, and
/// unsafe is confined to this module (AGENTS.md invariant 3, ADR 0009).
pub struct SingleInstance(HANDLE);

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: the handle was created by acquire_single_instance and is
        // closed exactly once here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// `Ok(None)` means another instance already holds the mutex. The `Local\`
/// namespace scopes it per login session on purpose: hooks are per-session,
/// so two different sessions may each run their own winremap.
pub fn acquire_single_instance() -> windows::core::Result<Option<SingleInstance>> {
    // SAFETY: the name is a static wide string; the returned handle is owned
    // by SingleInstance and closed on drop.
    let handle = unsafe { CreateMutexW(None, false, w!("Local\\winremap-single-instance")) }?;
    // CreateMutexW succeeds even when the mutex exists; only the last-error
    // state distinguishes "created" from "opened someone else's".
    // SAFETY: reads calling thread's last-error slot, set by the call above.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        drop(SingleInstance(handle));
        Ok(None)
    } else {
        Ok(Some(SingleInstance(handle)))
    }
}

/// Pumps this thread's message queue until WM_QUIT. Both the keyboard hook
/// and the WinEvent hook are serviced by this loop; `on_message` runs after
/// each dispatched message so the tray can drain its event channel without a
/// second thread.
pub fn run_message_loop(mut on_message: impl FnMut()) {
    let mut msg = MSG::default();
    // SAFETY: msg is a live local; a null HWND means "all messages of this
    // thread", which hook dispatch requires. `.0 > 0` also stops on the -1
    // error return, which as_bool() would misread as "keep going".
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        // SAFETY: msg was filled in by the successful GetMessageW above.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        on_message();
    }
}

/// Asks the message loop to exit (used by the tray's Quit item).
pub fn post_quit() {
    // SAFETY: no preconditions; posts WM_QUIT to the calling thread's queue.
    unsafe { PostQuitMessage(0) };
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, Windows guarantees lparam points at a valid
        // KBDLLHOOKSTRUCT for the duration of this call.
        let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if handle_event(wparam.0 as u32, event) {
            return LRESULT(1); // non-zero suppresses delivery of the event
        }
    }
    // SAFETY: forwarding unchanged arguments as required by the hook chain
    // contract.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Returns `true` when the original event must be suppressed.
fn handle_event(message: u32, event: &KBDLLHOOKSTRUCT) -> bool {
    let injected = event.flags.contains(LLKHF_INJECTED);
    let vk = event.vkCode as u16;
    let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;

    if injected {
        // Injected events pass through untouched — remapping them could loop
        // (AGENTS.md invariant 1). Our own remap output still updates the
        // logical modifier state so remapped modifiers can form chords.
        if event.dwExtraInfo == sender::MARKER_REMAP
            && let Some(bit) = sender::side_bit(vk)
        {
            update_sides(bit, down);
        }
        return false;
    }

    if let Some(bit) = sender::side_bit(vk) {
        // Modifier keys are not remappable in v0.1 (config rejects them as
        // inputs), so they only feed the state used to match other keys.
        update_sides(bit, down);
        return false;
    }

    if down { on_key_down(vk) } else { on_key_up(vk) }
}

fn update_sides(bit: SideMods, down: bool) {
    SIDES.with(|sides| {
        let current = sides.get();
        sides.set(if down { current | bit } else { current & !bit });
    });
}

fn on_key_down(vk: u16) -> bool {
    // Auto-repeat: keep emitting the target chosen at the initial press even
    // if modifiers drifted since — releasing Ctrl mid-repeat must not morph
    // a remapped C-h back into plain h halfway through.
    let repeating = ACTIVE.with(|active| active.borrow()[usize::from(vk)]);
    if let Some(remap) = repeating {
        match remap.kind {
            RemapKind::Exact => sender::send_exact_repeat(remap.target.vk),
            RemapKind::KeyOnly => sender::send_key_only(remap.target.vk, false),
        }
        return true;
    }

    if !ENABLED.load(Ordering::Relaxed) {
        return false;
    }

    let sides = SIDES.with(Cell::get);
    let input = KeyCombo {
        mods: sender::side_mods_to_mods(sides),
        vk,
    };
    let table = REMAP_TABLE.load();
    let Some(table) = table.as_ref() else {
        return false;
    };
    let Some(action) = crate::window::with_foreground_exe(|exe| table.resolve(exe, input)) else {
        return false;
    };

    let adjustment = match action.kind {
        RemapKind::Exact => sender::send_exact_down(action.target, sides),
        RemapKind::KeyOnly => {
            sender::send_key_only(action.target.vk, false);
            ModAdjustment::default()
        }
    };
    ACTIVE.with(|active| {
        active.borrow_mut()[usize::from(vk)] = Some(ActiveRemap {
            target: action.target,
            kind: action.kind,
            adjustment,
        });
    });
    true
}

fn on_key_up(vk: u16) -> bool {
    let remap = ACTIVE.with(|active| active.borrow_mut()[usize::from(vk)].take());
    let Some(remap) = remap else {
        // Not a key we remapped (or its press predated the hook): let the
        // original key-up through so applications never see a stuck key.
        return false;
    };
    match remap.kind {
        RemapKind::Exact => {
            let still_held = SIDES.with(Cell::get);
            sender::send_exact_up(remap.target, remap.adjustment, still_held);
        }
        RemapKind::KeyOnly => sender::send_key_only(remap.target.vk, true),
    }
    true
}
