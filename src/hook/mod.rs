//! Low-level keyboard hook: the only place key events are intercepted.
//!
//! The callback is the latency-critical path (AGENTS.md invariant 2): it only
//! reads thread-local state and the atomically-swapped remap table — no
//! allocation, no locking, no I/O, and no panics (a panic would unwind across
//! the FFI boundary into Windows).
//!
//! State lives in `thread_local`s because a WH_KEYBOARD_LL callback always
//! runs on the thread that installed it, via its message loop.

//!
//! Split by what each file is for (ADR 0084): this file is the callback and
//! the state it reads; `debug` queues and formats the `--debug` commentary;
//! `record` carries recording events to the macro-record feature;
//! `instance` is the single-instance mutex; `message_loop` pumps the thread
//! the hook runs on.

mod debug;
mod instance;
mod message_loop;
mod record;

pub use debug::{drain_debug_log, log_foreground};
pub use instance::acquire_single_instance;
pub use message_loop::{post_quit, run_message_loop, wake_message_loop};
pub use record::{abort_recording, drain_record_events};

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};

use arc_swap::ArcSwapOption;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use crate::i18n;
use crate::sender;
use crate::sender::{ModAdjustment, SideMods};
use debug::{DebugAction, DebugRing, InjectedSource, debug_action_of, log_debug};
use record::{RecordEventRing, RecordNote, queue_record_event, record_command, record_output};
use winremap::keymap::{KeyCombo, Mods, Output, RemapTable, Resolution};
use winremap::recorder::{RecordEvent, Recorder};

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
        // Recording keys are only intercepted while enabled, so a recording
        // left running here could never be stopped (design doc §5.6).
        abort_recording(i18n::t().macro_record_reason_disabled);
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

/// `--accept-injected`: convert injected events that we did not inject
/// ourselves, so a UI-test agent's `SendInput` can be remapped (ADR 0053).
/// Only exists in `test-inject` builds; default OFF even there.
#[cfg(feature = "test-inject")]
static ACCEPT_INJECTED: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "test-inject")]
pub fn set_accept_injected(enabled: bool) {
    ACCEPT_INJECTED.store(enabled, Ordering::Relaxed);
}

/// Without the feature this is a compile-time `false`, so the shipped binary
/// keeps the unconditional pass-through of invariant 1 with no branch added.
#[inline]
pub fn accept_injected() -> bool {
    #[cfg(feature = "test-inject")]
    {
        ACCEPT_INJECTED.load(Ordering::Relaxed)
    }
    #[cfg(not(feature = "test-inject"))]
    {
        false
    }
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
    /// Physical down-state per VK, to tell auto-repeats from fresh presses
    /// of keys we do not remap (their repeats are not debug-logged so a held
    /// key cannot flood the log).
    static PHYS_DOWN: RefCell<[bool; 256]> = const { RefCell::new([false; 256]) };
    /// The in-progress macro recording (ADR 0043). Fixed-size and owned by
    /// this thread, so recording costs the callback a copy and nothing else.
    static RECORDER: RefCell<Recorder> = const { RefCell::new(Recorder::new()) };
    /// Recording events awaiting the message loop, which is where they can
    /// be logged and where the finished macro can be published (allocating).
    static RECORD_EVENTS: RefCell<RecordEventRing> = const { RefCell::new(RecordEventRing::new()) };
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
fn arm_menu_guard(mods: Mods) {
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
        let source = match event.dwExtraInfo {
            sender::MARKER_REMAP => InjectedSource::Remap,
            sender::MARKER_COMPENSATION => InjectedSource::Compensation,
            _ => InjectedSource::External,
        };
        // Our own remap output still updates the logical modifier state so
        // remapped modifiers can form chords.
        if event.dwExtraInfo == sender::MARKER_REMAP
            && let Some(bit) = sender::side_bit(vk)
        {
            update_sides(bit, down);
        }
        // Injected events pass through untouched — remapping them could loop
        // (AGENTS.md invariant 1). A `test-inject` build run with
        // `--accept-injected` converts *foreign* injections so the UI-test
        // agent's SendInput reaches the remap path; ours are still passed
        // through unconditionally, and that is what closes the loop (ADR 0053).
        let ours = !matches!(source, InjectedSource::External);
        if ours || !accept_injected() {
            if debug_enabled() {
                // Echo every injected event so the exact delivered stream —
                // including other software's injections — is visible (ADR 0016).
                log_debug(
                    None,
                    KeyCombo {
                        mods: Mods::NONE,
                        vk,
                    },
                    DebugAction::Injected {
                        vk,
                        up: !down,
                        source,
                    },
                );
            }
            return false;
        }
        // Falls through: from here the foreign injection is handled exactly
        // like a physical key, so the debug log shows the remap decision
        // rather than the pass-through echo.
    }

    // Memory-only bookkeeping (hook-safe): a down while already down is an
    // auto-repeat, which the pass-through debug log skips below.
    let physical_repeat = down && PHYS_DOWN.with(|held| held.borrow()[usize::from(vk)]);
    PHYS_DOWN.with(|held| held.borrow_mut()[usize::from(vk)] = down);

    // Logged before anything is decided, so the detailed view is a
    // transcript rather than a summary: modifiers, which no decision line
    // ever mentions, and releases, which is where the second half of a
    // remap's output comes from. Auto-repeat is left out — a held key fires
    // dozens of times a second and would bury the rest (ADR 0016's noise
    // rule); the `Repeat` line above already says a key is repeating.
    if !physical_repeat {
        log_debug(
            None,
            KeyCombo {
                mods: Mods::NONE,
                vk,
            },
            DebugAction::Physical { up: !down },
        );
    }

    if down {
        // IME indicator poke on toggle-candidate keys; bounded and
        // non-blocking, and the key always passes through unchanged
        // (invariant 2 exception 3, ADR 0020/0021).
        crate::ime_indicator::notify_keydown(KeyCombo {
            mods: sender::side_mods_to_mods(SIDES.with(Cell::get)),
            vk,
        });
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

    if down {
        on_key_down(vk, physical_repeat)
    } else {
        on_key_up(vk)
    }
}

fn update_sides(bit: SideMods, down: bool) {
    SIDES.with(|sides| {
        let current = sides.get();
        sides.set(if down { current | bit } else { current & !bit });
    });
}

fn on_key_down(vk: u16, physical_repeat: bool) -> bool {
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
        log_debug(
            None,
            KeyCombo {
                mods: sender::side_mods_to_mods(SIDES.with(Cell::get)),
                vk,
            },
            DebugAction::Repeat,
        );
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

    // Recording keys are taken before any keymap lookup and always
    // suppressed, so they never reach the application (invariant 4). The
    // config rejects a keymap rule on the same key, so nothing is being
    // shadowed silently here (design doc §4/§5.1). An armed prefix is left
    // armed: the next key can still complete the sequence.
    if let Some(keys) = table.macro_record.as_ref()
        && let Some(event) = RECORDER.with(|recorder| recorder.borrow_mut().on_key(input, keys))
    {
        // A refused replay must still say why: an unexplained dead key
        // press is exactly what a recording feature must not produce.
        let note = match event {
            // Refused either because nothing is recorded yet or because a
            // replay is still running; the two need different next steps.
            RecordEvent::Play if !crate::macro_record::request_replay(sides) => {
                if crate::macro_record::has_recording() {
                    RecordNote::Event(RecordEvent::Ignored)
                } else {
                    RecordNote::NothingToPlay
                }
            }
            other => RecordNote::Event(other),
        };
        queue_record_event(note);
        arm_menu_guard(input.mods);
        ACTIVE.with(|active| active.borrow_mut()[usize::from(vk)] = Some(ActiveKind::SuppressUp));
        return true;
    }

    if let Some(first) = PENDING.take() {
        // Second stroke of a sequence. Undefined combinations are swallowed
        // (Emacs-style) rather than passed through, so a typo after a prefix
        // cannot leak a stray keystroke into the application.
        let kind =
            match crate::window::with_foreground_exe(|exe| table.resolve_second(exe, first, input))
            {
                Some(output) => {
                    log_debug(Some(first), input, debug_action_of(output));
                    record_output(output);
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
            record_output(output);
            emit_output(output, sides)
        }
        Some(Resolution::KeyOnly(target_vk)) => {
            log_debug(None, input, DebugAction::KeyOnly(target_vk));
            // Bare substitution leaves the physical modifiers alone, so what
            // the application receives is the held modifiers plus the
            // substitute key — record that, not the bare key.
            record_command(KeyCombo {
                mods: input.mods,
                vk: target_vk,
            });
            sender::send_key_only(target_vk, false);
            ActiveKind::KeyOnly { target_vk }
        }
        Some(Resolution::Prefix) => {
            log_debug(None, input, DebugAction::Prefix);
            // A prefix emits nothing on its own; the second stroke's output
            // is what gets recorded.
            PENDING.set(Some(input));
            ActiveKind::SuppressUp
        }
        None => {
            // Log a passed key once per physical press: a held key (e.g. a
            // push-to-talk F1) repeats dozens of times per second and would
            // drown the log (ADR 0016's noise rule).
            if !physical_repeat {
                log_debug(None, input, DebugAction::Pass);
            }
            // A key WinRemap does not remap still reaches the application,
            // so it is part of what the recording has to reproduce. Repeats
            // are skipped for the same reason macros ignore them: a held key
            // must not fill the recording.
            if !physical_repeat {
                record_command(input);
            }
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
