//! The `--debug` commentary: queued inside the callback without allocating,
//! formatted on the message loop (ADR 0016, ADR 0057).

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP};

use super::{DEBUG_RING, debug_enabled};
use crate::gui::log::Kind;
use crate::i18n;
use winremap::keymap::{KeyCombo, MAX_MACRO_LEN, Mods, Output};

/// Who injected an event observed by the hook (debug echo, ADR 0016).
#[derive(Clone, Copy)]
pub(super) enum InjectedSource {
    Remap,
    Compensation,
    External,
}

/// What the hook decided for one key event, recorded for debug output.
#[derive(Clone, Copy)]
pub(super) enum DebugAction {
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
    /// A press or release as it arrived from the keyboard, logged before any
    /// decision is taken. Only the detailed view shows these (ADR 0057).
    Physical {
        up: bool,
    },
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
pub(super) struct DebugRing {
    events: [Option<DebugEvent>; DEBUG_RING_SIZE],
    len: usize,
    dropped: u32,
}

impl DebugRing {
    pub(super) const fn new() -> Self {
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

pub(super) fn log_debug(prev: Option<KeyCombo>, input: KeyCombo, action: DebugAction) {
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
    let texts = i18n::t();
    DEBUG_RING.with(|ring| {
        let mut ring = ring.borrow_mut();
        for event in ring.events.iter().take(ring.len).flatten() {
            // Kind decides whether the simple view keeps the line; the tag is
            // what the window puts in its gutter and the console prints in
            // front of the text (ADR 0057).
            let (kind, tag, line) = match event.action {
                DebugAction::Pass => (
                    Kind::Decision,
                    texts.log_tag_decision,
                    i18n::debug_key_pass(event.input),
                ),
                DebugAction::Chord(target) => (
                    Kind::Decision,
                    texts.log_tag_decision,
                    i18n::debug_key_chord(event.prev, event.input, target),
                ),
                DebugAction::KeyOnly(vk) => (
                    Kind::Decision,
                    texts.log_tag_decision,
                    i18n::debug_key_substituted(event.input, vk),
                ),
                DebugAction::Macro { elements, len } => {
                    // Joined here, outside the hook, where allocation is fine.
                    let steps = elements[..usize::from(len)]
                        .iter()
                        .map(crate::keyname::combo)
                        .collect::<Vec<_>>()
                        .join(" → ");
                    (
                        Kind::Decision,
                        texts.log_tag_decision,
                        i18n::debug_key_macro(event.prev, event.input, len, &steps),
                    )
                }
                DebugAction::Prefix => (
                    Kind::Decision,
                    texts.log_tag_decision,
                    i18n::debug_key_prefix(event.input),
                ),
                DebugAction::Swallow => (
                    Kind::Decision,
                    texts.log_tag_decision,
                    i18n::debug_key_swallowed(event.prev, event.input),
                ),
                // A decision, but one a held key makes dozens of times a
                // second. It belongs with the mechanics, or holding a key
                // buries everything else in the simple view.
                DebugAction::Repeat => (
                    Kind::Detail,
                    texts.log_tag_decision,
                    i18n::debug_key_repeat(event.input),
                ),
                DebugAction::Physical { up } => (
                    Kind::Detail,
                    texts.log_tag_input,
                    i18n::debug_physical(event.input.vk, up),
                ),
                DebugAction::Injected { vk, up, source } => {
                    let source = match source {
                        InjectedSource::Remap => texts.debug_source_remap,
                        InjectedSource::Compensation => texts.debug_source_compensation,
                        InjectedSource::External => texts.debug_source_external,
                    };
                    (
                        Kind::Detail,
                        texts.log_tag_injected,
                        i18n::debug_injected(vk, up, source),
                    )
                }
            };
            crate::gui::log::tagged(kind, tag, &line);
        }
        if ring.dropped > 0 {
            // An action rather than a note: it is stamped, so a reader can
            // see which burst of typing outran the buffer.
            crate::gui::log::action(&i18n::debug_events_dropped(ring.dropped));
        }
        ring.len = 0;
        ring.dropped = 0;
    });
}

pub(super) fn debug_action_of(output: &Output) -> DebugAction {
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
