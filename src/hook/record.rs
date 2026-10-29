//! Recording events on their way from the callback to the macro-record
//! feature: queued without allocating, published on the message loop
//! (ADR 0043, ADR 0044).

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP};

use super::{RECORD_EVENTS, RECORDER};
use crate::i18n;
use winremap::keymap::{KeyCombo, Output};
use winremap::recorder::{MAX_RECORDED_LEN, RecordEvent};

/// One recording's worth of `Recorded` events plus the start/stop pair, with
/// room to spare. Overflow is impossible in practice; dropping is still
/// preferable to growing inside the callback.
const RECORD_EVENT_RING_SIZE: usize = MAX_RECORDED_LEN + 8;

/// A recorder transition, or a refusal that only the macro-record side
/// knows about. Kept apart from [`RecordEvent`] because "nothing recorded
/// yet" is not a state of the recorder — it is a state of the stored macro.
#[derive(Clone, Copy)]
pub(super) enum RecordNote {
    Event(RecordEvent),
    NothingToPlay,
}

pub(super) struct RecordEventRing {
    events: [Option<RecordNote>; RECORD_EVENT_RING_SIZE],
    len: usize,
}

impl RecordEventRing {
    pub(super) const fn new() -> Self {
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
pub(super) fn queue_record_event(note: RecordNote) {
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
                crate::macro_record::banner_recording(0);
                crate::gui::log::action(&i18n::macro_record_started(MAX_RECORDED_LEN));
            }
            // The banner carries the progress; a log line per command would
            // only repeat what the key lines already say.
            RecordEvent::Recorded { len } => crate::macro_record::banner_recording(len),
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

/// Appends what a key press emitted to the recording, if one is running.
///
/// Recording stores the *remapped output* rather than the key pressed
/// (ADR 0043 decision 4), which is what lets a replay travel the same sender
/// path a config macro does. A macro output is stored as its individual
/// commands, so replaying it is indistinguishable from having typed them.
pub(super) fn record_output(output: &Output) {
    match output {
        Output::Chord(target) => record_command(*target),
        Output::Seq(sequence) => {
            for combo in sequence {
                record_command(*combo);
            }
        }
    }
}

pub(super) fn record_command(command: KeyCombo) {
    if let Some(event) = RECORDER.with(|recorder| recorder.borrow_mut().push(command)) {
        queue_record_event(RecordNote::Event(event));
    }
}
