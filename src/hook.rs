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
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, MSG, PostQuitMessage, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_APP, WM_KEYDOWN, WM_SYSKEYDOWN,
};
use windows::core::w;

use crate::i18n;
use crate::sender;
use crate::sender::{ModAdjustment, SideMods};
use winremap::keymap::{KeyCombo, Output, RemapTable, Resolution};

/// The active remap table. Written by the main/reload thread, read wait-free
/// from the hook (ADR 0003). `None` (before startup finishes) passes all
/// events through.
pub static REMAP_TABLE: ArcSwapOption<RemapTable> = ArcSwapOption::const_empty();

/// Tray toggle. Only gates *new* remaps: keys already remapped and held keep
/// their translation until release so no target key gets stuck down.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// Called from the tray on the hook's own thread, so it may also clear the
/// thread-local pending-prefix state.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        PENDING.set(None);
    }
}

/// `--debug` key logging. Default OFF (AGENTS.md invariant 6): output is
/// key-name level only and never persisted.
static DEBUG: AtomicBool = AtomicBool::new(false);

pub fn set_debug(enabled: bool) {
    DEBUG.store(enabled, Ordering::Relaxed);
}

pub fn debug_enabled() -> bool {
    DEBUG.load(Ordering::Relaxed)
}

/// What the hook decided for one key-down, recorded for debug output.
#[derive(Clone, Copy)]
enum DebugAction {
    Pass,
    Chord(KeyCombo),
    KeyOnly(u16),
    Macro(u8),
    Prefix,
    Swallow,
}

#[derive(Clone, Copy)]
struct DebugEvent {
    /// The armed prefix when this was a second stroke.
    prev: Option<KeyCombo>,
    input: KeyCombo,
    action: DebugAction,
}

const DEBUG_RING_SIZE: usize = 128;

/// Fixed-capacity event buffer: the callback must not print or allocate
/// (invariant 2), so events are queued here and formatted by
/// [`drain_debug_log`] on the message loop (ADR 0016).
struct DebugRing {
    events: [Option<DebugEvent>; DEBUG_RING_SIZE],
    len: usize,
    dropped: u32,
}

impl DebugRing {
    const fn new() -> Self {
        Self {
            events: [None; DEBUG_RING_SIZE],
            len: 0,
            dropped: 0,
        }
    }

    fn push(&mut self, event: DebugEvent) {
        if self.len < DEBUG_RING_SIZE {
            self.events[self.len] = Some(event);
            self.len += 1;
        } else {
            self.dropped += 1;
        }
    }
}

fn log_debug(prev: Option<KeyCombo>, input: KeyCombo, action: DebugAction) {
    if !debug_enabled() {
        return;
    }
    DEBUG_RING.with(|ring| {
        ring.borrow_mut().push(DebugEvent {
            prev,
            input,
            action,
        })
    });
    // Keystrokes do not queue messages for this thread, so GetMessageW would
    // sit idle and the log would only flush on the next unrelated message.
    // A cheap self-posted message wakes the loop; debug mode only.
    // SAFETY: posts to our own thread's queue; failure (queue full) only
    // delays output.
    let _ = unsafe { PostThreadMessageW(GetCurrentThreadId(), WM_APP, WPARAM(0), LPARAM(0)) };
}

/// Formats and prints queued debug events. Runs on the message loop, outside
/// the hook callback, where I/O and allocation are fine.
pub fn drain_debug_log() {
    if !debug_enabled() {
        return;
    }
    DEBUG_RING.with(|ring| {
        let mut ring = ring.borrow_mut();
        for event in ring.events.iter().take(ring.len).flatten() {
            let line = match event.action {
                DebugAction::Pass => i18n::debug_key_pass(event.input),
                DebugAction::Chord(target) => {
                    i18n::debug_key_chord(event.prev, event.input, target)
                }
                DebugAction::KeyOnly(vk) => i18n::debug_key_substituted(event.input, vk),
                DebugAction::Macro(len) => i18n::debug_key_macro(event.prev, event.input, len),
                DebugAction::Prefix => i18n::debug_key_prefix(event.input),
                DebugAction::Swallow => i18n::debug_key_swallowed(event.prev, event.input),
            };
            println!("{line}");
        }
        if ring.dropped > 0 {
            println!("{}", i18n::debug_events_dropped(ring.dropped));
        }
        ring.len = 0;
        ring.dropped = 0;
    });
}

/// What to do when a suppressed key's physical release (or repeat) arrives.
#[derive(Clone, Copy)]
enum ActiveKind {
    /// Chord output: release the target and undo the modifier surgery.
    Exact {
        target: KeyCombo,
        adjustment: ModAdjustment,
    },
    /// Bare-key substitution: release the substitute key.
    KeyOnly { target_vk: u16 },
    /// Nothing to emit — sequence prefixes, macro sources, and swallowed
    /// unmatched second strokes. Their key-up (and repeat) is suppressed so
    /// applications never see half of a consumed key.
    SuppressUp,
}

thread_local! {
    /// Logical side-modifier state: physical keys plus modifiers we injected
    /// as remap output (marker `MARKER_REMAP`), so CapsLock→LCtrl can still
    /// form Ctrl chords. Compensation events are deliberately not tracked.
    static SIDES: Cell<SideMods> = const { Cell::new(0) };
    /// Indexed by original VK. A flat array keeps lookup O(1) without hashing
    /// or allocation inside the callback.
    static ACTIVE: RefCell<[Option<ActiveKind>; 256]> = const { RefCell::new([None; 256]) };
    /// First stroke of a two-stroke sequence, armed until the next
    /// non-modifier key-down consumes it (ADR 0013). No timeout on purpose —
    /// Emacs prefixes wait indefinitely too.
    static PENDING: Cell<Option<KeyCombo>> = const { Cell::new(None) };
    /// Alt/Win classes whose chords we consumed while held. Their eventual
    /// physical release must be masked or Windows reads it as a lone tap and
    /// opens the menu bar / Start menu (ADR 0015).
    static MENU_GUARD: Cell<u8> = const { Cell::new(0) };
    /// Debug-mode event queue; drained by the message loop.
    static DEBUG_RING: RefCell<DebugRing> = const { RefCell::new(DebugRing::new()) };
}

const GUARD_ALT: u8 = 1;
const GUARD_WIN: u8 = 1 << 1;

fn guard_class(vk: u16) -> Option<u8> {
    match vk {
        0xA4 | 0xA5 => Some(GUARD_ALT), // LAlt / RAlt
        0x5B | 0x5C => Some(GUARD_WIN), // LWin / RWin
        _ => None,
    }
}

/// Arms the menu guard for the modifier classes involved in a suppressed
/// chord (see MENU_GUARD).
fn arm_menu_guard(mods: winremap::keymap::Mods) {
    use winremap::keymap::Mods;
    let mut bits = 0;
    if mods.contains(Mods::ALT) {
        bits |= GUARD_ALT;
    }
    if mods.contains(Mods::WIN) {
        bits |= GUARD_WIN;
    }
    if bits != 0 {
        MENU_GUARD.with(|guard| guard.set(guard.get() | bits));
    }
}

/// Returns whether `class` was armed, clearing it either way.
fn take_menu_guard(class: u8) -> bool {
    MENU_GUARD.with(|guard| {
        let armed = guard.get() & class != 0;
        if armed {
            guard.set(guard.get() & !class);
        }
        armed
    })
}

fn debug_action_of(output: &Output) -> DebugAction {
    match output {
        Output::Chord(target) => DebugAction::Chord(*target),
        Output::Seq(sequence) => DebugAction::Macro(sequence.len() as u8),
    }
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
        // Modifier keys are not remappable (config rejects them as inputs),
        // so they only feed the chord state used to match other keys.
        if !down
            && let Some(class) = guard_class(vk)
            && take_menu_guard(class)
        {
            // The mask must land before the release, so suppress the
            // physical up and emit [mask, up] as one ordered batch; the
            // injected up carries MARKER_REMAP and updates SIDES when it
            // comes back through the hook (ADR 0015).
            sender::send_masked_modifier_up(vk);
            return true;
        }
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
    // a remapped C-h back into plain h halfway through. Macro sources and
    // prefixes repeat as nothing: a held key must not spray macros.
    let repeating = ACTIVE.with(|active| active.borrow()[usize::from(vk)]);
    if let Some(kind) = repeating {
        match kind {
            ActiveKind::Exact { target, .. } => sender::send_exact_repeat(target.vk),
            ActiveKind::KeyOnly { target_vk } => sender::send_key_only(target_vk, false),
            ActiveKind::SuppressUp => {}
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

    if let Some(first) = PENDING.take() {
        // Second stroke of a sequence. Undefined combinations are swallowed
        // (Emacs-style) rather than passed through, so a typo after a prefix
        // cannot leak a stray keystroke into the application.
        let kind =
            match crate::window::with_foreground_exe(|exe| table.resolve_second(exe, first, input))
            {
                Some(output) => {
                    log_debug(Some(first), input, debug_action_of(output));
                    emit_output(output, sides)
                }
                None => {
                    log_debug(Some(first), input, DebugAction::Swallow);
                    ActiveKind::SuppressUp
                }
            };
        arm_menu_guard(input.mods);
        ACTIVE.with(|active| active.borrow_mut()[usize::from(vk)] = Some(kind));
        return true;
    }

    let resolution = crate::window::with_foreground_exe(|exe| table.resolve(exe, input));
    let kind = match resolution {
        Some(Resolution::Exact(output)) => {
            log_debug(None, input, debug_action_of(output));
            emit_output(output, sides)
        }
        Some(Resolution::KeyOnly(target_vk)) => {
            log_debug(None, input, DebugAction::KeyOnly(target_vk));
            sender::send_key_only(target_vk, false);
            ActiveKind::KeyOnly { target_vk }
        }
        Some(Resolution::Prefix) => {
            log_debug(None, input, DebugAction::Prefix);
            PENDING.set(Some(input));
            ActiveKind::SuppressUp
        }
        None => {
            log_debug(None, input, DebugAction::Pass);
            return false;
        }
    };
    // A consumed Alt/Win chord means the eventual physical modifier release
    // must be masked (ADR 0015).
    arm_menu_guard(input.mods);
    ACTIVE.with(|active| active.borrow_mut()[usize::from(vk)] = Some(kind));
    true
}

/// Sends a resolved output and returns the bookkeeping for its key-up.
fn emit_output(output: &Output, sides: SideMods) -> ActiveKind {
    match output {
        Output::Chord(target) => {
            let adjustment = sender::send_exact_down(*target, sides);
            ActiveKind::Exact {
                target: *target,
                adjustment,
            }
        }
        Output::Seq(sequence) => {
            // Macros complete (downs and ups) within this press; the source
            // key-up has nothing left to do.
            sender::send_sequence(sequence, sides);
            ActiveKind::SuppressUp
        }
    }
}

fn on_key_up(vk: u16) -> bool {
    let remap = ACTIVE.with(|active| active.borrow_mut()[usize::from(vk)].take());
    let Some(kind) = remap else {
        // Not a key we remapped (or its press predated the hook): let the
        // original key-up through so applications never see a stuck key.
        return false;
    };
    match kind {
        ActiveKind::Exact { target, adjustment } => {
            let still_held = SIDES.with(Cell::get);
            sender::send_exact_up(target, adjustment, still_held);
        }
        ActiveKind::KeyOnly { target_vk } => sender::send_key_only(target_vk, true),
        ActiveKind::SuppressUp => {}
    }
    true
}
