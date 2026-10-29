//! The `--debug` commentary: queued inside the callback without allocating,
//! formatted on the message loop (ADR 0016, ADR 0057).

use super::DEBUG_RING;
use crate::gui::log::Kind;
use crate::i18n;
use winremap::engine::{DebugAction, DebugEvent, Injected};

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

/// Queues one line for the message loop and wakes it. Hook-safe: a write
/// into a pre-allocated array plus one non-blocking self-post.
pub(super) fn log_debug(event: DebugEvent) {
    DEBUG_RING.with(|ring| ring.borrow_mut().push(event));
    // Keystrokes do not queue messages for this thread, so GetMessageW would
    // sit idle and the log would only flush on the next unrelated message.
    // A cheap self-posted message wakes the loop; debug mode only.
    super::message_loop::wake_self();
}

/// Formats and prints queued debug events. Runs on the message loop, outside
/// the hook callback, where I/O and allocation are fine.
pub fn drain_debug_log() {
    if !super::debug_enabled() {
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
                        Injected::Remap => texts.debug_source_remap,
                        Injected::Compensation => texts.debug_source_compensation,
                        Injected::External => texts.debug_source_external,
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

/// The `[前面]` lines after a foreground change, called from `window.rs` when
/// `--debug` is on: the exact `application` value to use, which configured
/// keymaps would apply, and the full path.
///
/// Tagged and stamped like the key traffic it sits among (ADR 0058). It used
/// to be three untimed lines led by `[debug]`, which is how a report about
/// the window the user just clicked came to look like part of the previous
/// keystroke (owner request 2026-07-29).
pub fn log_foreground(full_path: Option<&str>, basename: &str) {
    let texts = i18n::t();
    let Some(full_path) = full_path else {
        crate::gui::log::tagged(
            Kind::Action,
            texts.log_tag_window,
            texts.debug_foreground_unknown,
        );
        return;
    };
    let table = super::REMAP_TABLE.load();
    let names: Vec<&str> = table
        .as_ref()
        .map(|t| {
            t.keymaps
                .iter()
                .filter(|k| k.apps.matches(basename))
                .map(|k| k.name.as_str())
                .collect()
        })
        .unwrap_or_default();
    let list = if names.is_empty() {
        texts.debug_none.to_string()
    } else {
        names.join(", ")
    };
    crate::gui::log::tagged(
        Kind::Action,
        texts.log_tag_window,
        &i18n::debug_foreground(basename, &list),
    );
    // The path is the least-read part of the report and the longest, so it
    // goes under the summary, where the detailed view can hide it.
    crate::gui::log::tagged(Kind::Detail, texts.log_tag_window, full_path);
}
