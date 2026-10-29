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
//! What a key event *means* is decided by the library's
//! [`Engine`](winremap::engine::Engine) (ADR 0086): this module owns the
//! one engine, translates each event for it, and applies what it decides —
//! `SendInput` through `sender`, the `--debug` ring (`debug`), the recording
//! ring (`record`), the IME indicator's poke. Nothing here decides anything
//! about keys. The rest of the directory (ADR 0084): `instance` is the
//! single-instance mutex and `message_loop` pumps the thread the hook runs
//! on.

mod debug;
mod instance;
mod message_loop;
mod record;

pub use debug::{drain_debug_log, log_foreground};
pub use instance::acquire_single_instance;
pub use message_loop::{post_quit, run_message_loop, wake_message_loop};
pub use record::{abort_recording, drain_record_events};

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use arc_swap::ArcSwapOption;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use crate::i18n;
use crate::sender;
use debug::{DebugRing, log_debug};
use record::{Queued, RecordEventRing, queue_record_event};
use winremap::engine::{Emit, Engine, Injected, KeyEvent, Options, RecordNote};
use winremap::keymap::RemapTable;
use winremap::recorder::RecordEvent;

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
        ENGINE.with(|engine| engine.borrow_mut().clear_pending());
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

thread_local! {
    /// The decision-maker. Everything it needs to remember between events
    /// — modifiers, keys held under a remap, an armed prefix, the menu
    /// guard, the recording — is inside it (ADR 0086).
    static ENGINE: RefCell<Engine> = const { RefCell::new(Engine::new()) };
    /// Debug-mode event queue; drained by the message loop.
    static DEBUG_RING: RefCell<DebugRing> = const { RefCell::new(DebugRing::new()) };
    /// Recording events awaiting the message loop, which is where they can
    /// be logged and where the finished macro can be published (allocating).
    static RECORD_EVENTS: RefCell<RecordEventRing> = const { RefCell::new(RecordEventRing::new()) };
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
///
/// Translates the Win32 event for the engine, lets it decide, and applies
/// the decision in the order the callback always had: the debug lines, the
/// IME poke, the recording notes, then the keys sent.
fn handle_event(message: u32, event: &KBDLLHOOKSTRUCT) -> bool {
    let injected = if event.flags.contains(LLKHF_INJECTED) {
        Some(match event.dwExtraInfo {
            sender::MARKER_REMAP => Injected::Remap,
            sender::MARKER_COMPENSATION => Injected::Compensation,
            _ => Injected::External,
        })
    } else {
        None
    };
    let key_event = KeyEvent {
        vk: event.vkCode as u16,
        down: message == WM_KEYDOWN || message == WM_SYSKEYDOWN,
        injected,
    };
    let options = Options {
        enabled: ENABLED.load(Ordering::Relaxed),
        accept_injected: accept_injected(),
    };

    let table = REMAP_TABLE.load();
    let decision = crate::window::with_foreground_exe(|exe| {
        ENGINE.with(|engine| {
            engine
                .borrow_mut()
                .on_event(key_event, table.as_deref(), exe, options)
        })
    });

    if debug_enabled() {
        for line in decision.debug_events() {
            log_debug(*line);
        }
    }
    if let Some(input) = decision.ime_poke {
        // Bounded and non-blocking (invariant 2 exception 3, ADR 0020/0021).
        crate::ime_indicator::notify_keydown(input);
    }
    for note in decision.record_notes() {
        queue_record_event(match *note {
            RecordNote::Event(event) => Queued::Event(event),
            // A refused replay must still say why: an unexplained dead key
            // press is exactly what a recording feature must not produce.
            // Refused either because nothing is recorded yet or because a
            // replay is still running; the two need different next steps.
            RecordNote::Play { held } => {
                if crate::macro_record::request_replay(held) {
                    Queued::Event(RecordEvent::Play)
                } else if crate::macro_record::has_recording() {
                    Queued::Event(RecordEvent::Ignored)
                } else {
                    Queued::NothingToPlay
                }
            }
        });
    }
    if let Some(emit) = decision.emit {
        apply(emit);
    }
    decision.suppress
}

/// The sender call a decision asks for.
fn apply(emit: Emit<'_>) {
    match emit {
        Emit::ExactDown { target, adjustment } => sender::send_exact_down(target, adjustment),
        Emit::ExactRepeat(vk) => sender::send_exact_repeat(vk),
        Emit::ExactUp {
            target,
            adjustment,
            still_held,
        } => sender::send_exact_up(target, adjustment, still_held),
        Emit::KeyOnly { vk, up } => sender::send_key_only(vk, up),
        Emit::MaskedModifierUp(vk) => sender::send_masked_modifier_up(vk),
        Emit::Sequence(sequence) => {
            let held = ENGINE.with(|engine| engine.borrow().sides());
            sender::send_sequence(sequence, held);
        }
    }
}
