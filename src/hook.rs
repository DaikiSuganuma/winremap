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
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

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
use winremap::keymap::{KeyCombo, MAX_MACRO_LEN, Mods, Output, RemapTable, Resolution};
use winremap::recorder::{MAX_RECORDED_LEN, RecordEvent, Recorder};

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

/// Who injected an event observed by the hook (debug echo, ADR 0016).
#[derive(Clone, Copy)]
enum InjectedSource {
    Remap,
    Compensation,
    External,
}

/// What the hook decided for one key event, recorded for debug output.
#[derive(Clone, Copy)]
enum DebugAction {
    Pass,
    Chord(KeyCombo),
    KeyOnly(u16),
    Macro {
        elements: [KeyCombo; MAX_MACRO_LEN],
        len: u8,
    },
    Prefix,
    Swallow,
    Repeat,
    Injected {
        vk: u16,
        up: bool,
        source: InjectedSource,
    },
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
                DebugAction::Macro { elements, len } => {
                    // Joined here, outside the hook, where allocation is fine.
                    let steps = elements[..usize::from(len)]
                        .iter()
                        .map(|combo| combo.to_string())
                        .collect::<Vec<_>>()
                        .join(" → ");
                    i18n::debug_key_macro(event.prev, event.input, len, &steps)
                }
                DebugAction::Prefix => i18n::debug_key_prefix(event.input),
                DebugAction::Swallow => i18n::debug_key_swallowed(event.prev, event.input),
                DebugAction::Repeat => i18n::debug_key_repeat(event.input),
                DebugAction::Injected { vk, up, source } => {
                    let source = match source {
                        InjectedSource::Remap => i18n::t().debug_source_remap,
                        InjectedSource::Compensation => i18n::t().debug_source_compensation,
                        InjectedSource::External => i18n::t().debug_source_external,
                    };
                    i18n::debug_injected(vk, up, source)
                }
            };
            crate::gui::log::emit(&line);
        }
        if ring.dropped > 0 {
            crate::gui::log::emit(&i18n::debug_events_dropped(ring.dropped));
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

/// One recording's worth of `Recorded` events plus the start/stop pair, with
/// room to spare. Overflow is impossible in practice; dropping is still
/// preferable to growing inside the callback.
const RECORD_EVENT_RING_SIZE: usize = MAX_RECORDED_LEN + 8;

/// A recorder transition, or a refusal that only the macro-record side
/// knows about. Kept apart from [`RecordEvent`] because "nothing recorded
/// yet" is not a state of the recorder — it is a state of the stored macro.
#[derive(Clone, Copy)]
enum RecordNote {
    Event(RecordEvent),
    NothingToPlay,
}

struct RecordEventRing {
    events: [Option<RecordNote>; RECORD_EVENT_RING_SIZE],
    len: usize,
}

impl RecordEventRing {
    const fn new() -> Self {
        Self {
            events: [None; RECORD_EVENT_RING_SIZE],
            len: 0,
        }
    }
}

/// Queues a recording event for the message loop and wakes it.
///
/// Hook-safe: a write into a pre-allocated array plus one non-blocking
/// self-post (invariant 2, exception 4). The wake is needed because
/// keystrokes queue no message for this thread, so `GetMessageW` would
/// otherwise sit idle and the banner would lag behind the typing.
fn queue_record_event(note: RecordNote) {
    RECORD_EVENTS.with(|ring| {
        let mut ring = ring.borrow_mut();
        let len = ring.len;
        if len < RECORD_EVENT_RING_SIZE {
            ring.events[len] = Some(note);
            ring.len = len + 1;
        }
    });
    // SAFETY: posts to our own thread's queue; failure (queue full) only
    // delays the log line and the banner update.
    let _ = unsafe { PostThreadMessageW(GetCurrentThreadId(), WM_APP, WPARAM(0), LPARAM(0)) };
}

/// Handles queued recording events on the message loop: logging, and
/// publishing a finished recording to the macro-record thread. Runs outside
/// the hook callback, where allocation and I/O are fine.
pub fn drain_record_events() {
    let events = RECORD_EVENTS.with(|ring| {
        let mut ring = ring.borrow_mut();
        let taken = ring.len;
        ring.len = 0;
        (0..taken)
            .filter_map(|i| ring.events[i])
            .collect::<Vec<_>>()
    });
    for note in events {
        let event = match note {
            RecordNote::Event(event) => event,
            RecordNote::NothingToPlay => {
                crate::gui::log::action(&i18n::macro_record_nothing_to_play());
                continue;
            }
        };
        match event {
            RecordEvent::Started => {
                crate::macro_record::banner_show(i18n::macro_record_banner_recording(
                    0,
                    MAX_RECORDED_LEN,
                ));
                crate::gui::log::action(&i18n::macro_record_started(MAX_RECORDED_LEN));
            }
            // The banner carries the progress; a log line per command would
            // only repeat what the key lines already say.
            RecordEvent::Recorded { len } => {
                crate::macro_record::banner_show(i18n::macro_record_banner_recording(
                    len,
                    MAX_RECORDED_LEN,
                ));
            }
            RecordEvent::Stopped { len } => {
                publish_recording();
                crate::macro_record::banner_hide();
                crate::gui::log::action(&i18n::macro_record_stopped(len));
            }
            RecordEvent::Truncated { .. } => {
                publish_recording();
                crate::macro_record::banner_notice(i18n::macro_record_banner_limit(
                    MAX_RECORDED_LEN,
                ));
                crate::gui::log::action(&i18n::macro_record_truncated(MAX_RECORDED_LEN));
            }
            RecordEvent::Play => {}
            RecordEvent::Ignored => {
                crate::gui::log::action(&i18n::macro_record_ignored());
            }
        }
    }
}

/// Copies the finished recording out of the hook's thread-local buffer.
/// Only safe to call from this thread, which the message loop is.
fn publish_recording() {
    RECORDER.with(|recorder| crate::macro_record::publish(recorder.borrow().recorded()));
}

/// Drops an in-progress recording. Called when the tray disables remapping
/// or a reload lands: the keys that would end the recording come from the
/// config, so leaving it running risks a recording that cannot be stopped
/// (design doc §5.6). Must run on the hook thread — both callers already do.
pub fn abort_recording(reason: &str) {
    let was_recording = RECORDER.with(|recorder| {
        let mut recorder = recorder.borrow_mut();
        let active = recorder.is_recording();
        recorder.abort();
        active
    });
    if was_recording {
        crate::macro_record::banner_notice(i18n::macro_record_aborted(reason));
        crate::gui::log::action(&i18n::macro_record_aborted(reason));
    }
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

fn debug_action_of(output: &Output) -> DebugAction {
    match output {
        Output::Chord(target) => DebugAction::Chord(*target),
        Output::Seq(sequence) => {
            let mut elements = [KeyCombo {
                mods: Mods::NONE,
                vk: 0,
            }; MAX_MACRO_LEN];
            for (slot, combo) in elements.iter_mut().zip(sequence) {
                *slot = *combo;
            }
            DebugAction::Macro {
                elements,
                len: sequence.len().min(MAX_MACRO_LEN) as u8,
            }
        }
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
    // SAFETY: no preconditions; returns the calling thread's own id.
    MESSAGE_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
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

/// The message loop's thread, so other threads can wake it. Zero until the
/// loop starts.
static MESSAGE_THREAD: AtomicU32 = AtomicU32::new(0);

/// Wakes the message loop from another thread.
///
/// `GetMessageW` blocks, and nothing queues a message for this thread while
/// the user is not typing — so a flag set on the GUI thread (the settings
/// window's reload button) would sit unnoticed until some unrelated event
/// arrived. The message itself carries nothing; waking is the point.
pub fn wake_message_loop() {
    let thread = MESSAGE_THREAD.load(Ordering::SeqCst);
    if thread == 0 {
        return;
    }
    // SAFETY: posting to a thread id is safe even if that thread has exited —
    // the call just fails, and the caller's request is dropped with it.
    let _ = unsafe { PostThreadMessageW(thread, WM_APP, WPARAM(0), LPARAM(0)) };
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
        if debug_enabled() {
            // Echo every injected event so the exact delivered stream —
            // including other software's injections — is visible (ADR 0016).
            let source = match event.dwExtraInfo {
                sender::MARKER_REMAP => InjectedSource::Remap,
                sender::MARKER_COMPENSATION => InjectedSource::Compensation,
                _ => InjectedSource::External,
            };
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
        if event.dwExtraInfo == sender::MARKER_REMAP
            && let Some(bit) = sender::side_bit(vk)
        {
            update_sides(bit, down);
        }
        return false;
    }

    // Memory-only bookkeeping (hook-safe): a down while already down is an
    // auto-repeat, which the pass-through debug log skips below.
    let physical_repeat = down && PHYS_DOWN.with(|held| held.borrow()[usize::from(vk)]);
    PHYS_DOWN.with(|held| held.borrow_mut()[usize::from(vk)] = down);

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

/// Appends what a key press emitted to the recording, if one is running.
///
/// Recording stores the *remapped output* rather than the key pressed
/// (ADR 0043 decision 4), which is what lets a replay travel the same sender
/// path a config macro does. A macro output is stored as its individual
/// commands, so replaying it is indistinguishable from having typed them.
fn record_output(output: &Output) {
    match output {
        Output::Chord(target) => record_command(*target),
        Output::Seq(sequence) => {
            for combo in sequence {
                record_command(*combo);
            }
        }
    }
}

fn record_command(command: KeyCombo) {
    if let Some(event) = RECORDER.with(|recorder| recorder.borrow_mut().push(command)) {
        queue_record_event(RecordNote::Event(event));
    }
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
