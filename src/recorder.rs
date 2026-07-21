//! Runtime macro recording: the keys that drive it and the state machine
//! that owns the recorded commands (docs/v0.3/02_macro-record-design.md,
//! ADR 0043).
//!
//! Only the pure side of the feature lives in the library so it is testable
//! on headless CI; the Win32 side (banner window, replay thread) is a
//! separate module tree in the binary, `src/macro_record/` (ADR 0044).
//!
//! The recorder is driven from inside the hook callback, so every method
//! here must stay allocation-free and lock-free (invariant 2): the buffer is
//! a fixed-size array reserved up front, and a recorded command is a `Copy`
//! [`KeyCombo`].

use crate::keymap::KeyCombo;

/// How many commands one recording holds (ADR 0043 decision 5). A "command"
/// is one chord, so `C-a` is one and the two strokes of `C-x C-s` are two.
///
/// This bound is what makes the fixed-size buffer below possible; raising it
/// also lengthens a replay, which ADR 0044 keeps off the hook thread for
/// exactly that reason.
pub const MAX_RECORDED_LEN: usize = 20;

/// The three keys that drive recording, resolved from `[macro]`. `stop`
/// equals `start` when the user omitted it, which is the default
/// "same key starts and ends" behaviour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RecordKeys {
    pub start: KeyCombo,
    pub stop: KeyCombo,
    pub play: KeyCombo,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecordState {
    Idle,
    Recording,
}

/// What pressing a key did to the recorder. The hook turns these into log
/// lines and banner updates on the message loop, never inside the callback.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecordEvent {
    /// Recording began; the buffer is now empty.
    Started,
    /// One command was appended.
    Recorded { len: usize },
    /// The stop key ended recording; `len` commands are ready to publish.
    Stopped { len: usize },
    /// The limit was hit, so recording ended by itself keeping the first
    /// [`MAX_RECORDED_LEN`] commands (owner decision: truncate and tell).
    Truncated { len: usize },
    /// The play key was pressed while idle.
    Play,
    /// A record key that means nothing in the current state: the play key
    /// while recording (nesting would make the recording ambiguous), the
    /// start key while already recording, or the stop key while idle.
    Ignored,
}

/// Fixed-capacity recording buffer plus its state. Lives in a
/// `thread_local` on the hook thread, so it needs no synchronization.
pub struct Recorder {
    state: RecordState,
    buffer: [KeyCombo; MAX_RECORDED_LEN],
    len: usize,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    pub const fn new() -> Self {
        Self {
            state: RecordState::Idle,
            // Reserved up front so `push` never allocates (invariant 2).
            buffer: [KeyCombo {
                mods: crate::keymap::Mods::NONE,
                vk: 0,
            }; MAX_RECORDED_LEN],
            len: 0,
        }
    }

    pub fn state(&self) -> RecordState {
        self.state
    }

    pub fn is_recording(&self) -> bool {
        self.state == RecordState::Recording
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The commands recorded so far.
    pub fn recorded(&self) -> &[KeyCombo] {
        &self.buffer[..self.len]
    }

    /// Feeds a key press to the recorder before any keymap lookup happens.
    ///
    /// `Some(event)` means this was a record key: the hook must suppress the
    /// event so it never reaches the application (invariant 4). `None` means
    /// it was an ordinary key and normal resolution should continue.
    pub fn on_key(&mut self, input: KeyCombo, keys: &RecordKeys) -> Option<RecordEvent> {
        // Stop is checked before start so a single key can serve as both:
        // while recording it ends, while idle it begins.
        if self.is_recording() && input == keys.stop {
            self.state = RecordState::Idle;
            return Some(RecordEvent::Stopped { len: self.len });
        }
        if input == keys.start {
            if self.is_recording() {
                return Some(RecordEvent::Ignored);
            }
            self.state = RecordState::Recording;
            self.len = 0;
            return Some(RecordEvent::Started);
        }
        if input == keys.play {
            // Recording a replay would blur what the recording means, and
            // the replayed commands would be recorded twice over.
            if self.is_recording() {
                return Some(RecordEvent::Ignored);
            }
            return Some(RecordEvent::Play);
        }
        if input == keys.stop {
            // Stop while idle: still a record key, so it must not leak
            // through to the application.
            return Some(RecordEvent::Ignored);
        }
        None
    }

    /// Appends one command of remapped output. Does nothing unless recording.
    pub fn push(&mut self, command: KeyCombo) -> Option<RecordEvent> {
        if !self.is_recording() {
            return None;
        }
        if self.len == MAX_RECORDED_LEN {
            // The command that would overflow ends the recording; the first
            // MAX_RECORDED_LEN stay. The key itself still does whatever it
            // normally does — recording is passive.
            self.state = RecordState::Idle;
            return Some(RecordEvent::Truncated { len: self.len });
        }
        self.buffer[self.len] = command;
        self.len += 1;
        Some(RecordEvent::Recorded { len: self.len })
    }

    /// Drops an in-progress recording (tray disable, config reload). The
    /// stop key comes from the config, so a reload can leave a recording
    /// with no way to end it — better to abandon it than to strand it
    /// (design doc §5.6).
    pub fn abort(&mut self) {
        self.state = RecordState::Idle;
        self.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::parse_key_combo;

    fn combo(spec: &str) -> KeyCombo {
        parse_key_combo(spec).unwrap()
    }

    /// The default shape: one key toggles recording, another replays.
    fn toggle_keys() -> RecordKeys {
        RecordKeys {
            start: combo("S-F10"),
            stop: combo("S-F10"),
            play: combo("F10"),
        }
    }

    fn separate_keys() -> RecordKeys {
        RecordKeys {
            start: combo("S-F10"),
            stop: combo("S-F11"),
            play: combo("F10"),
        }
    }

    #[test]
    fn records_and_replays_a_macro() {
        let keys = toggle_keys();
        let mut rec = Recorder::new();

        assert_eq!(rec.on_key(keys.start, &keys), Some(RecordEvent::Started));
        assert!(rec.is_recording());
        assert_eq!(
            rec.push(combo("C-a")),
            Some(RecordEvent::Recorded { len: 1 })
        );
        assert_eq!(
            rec.push(combo("C-k")),
            Some(RecordEvent::Recorded { len: 2 })
        );
        assert_eq!(
            rec.on_key(keys.stop, &keys),
            Some(RecordEvent::Stopped { len: 2 })
        );
        assert!(!rec.is_recording());
        assert_eq!(rec.recorded(), [combo("C-a"), combo("C-k")]);
        assert_eq!(rec.on_key(keys.play, &keys), Some(RecordEvent::Play));
        // Replaying does not consume the recording.
        assert_eq!(rec.on_key(keys.play, &keys), Some(RecordEvent::Play));
        assert_eq!(rec.recorded().len(), 2);
    }

    #[test]
    fn separate_stop_key_ends_recording() {
        let keys = separate_keys();
        let mut rec = Recorder::new();
        rec.on_key(keys.start, &keys);
        rec.push(combo("C-a"));
        // The start key pressed again while recording changes nothing.
        assert_eq!(rec.on_key(keys.start, &keys), Some(RecordEvent::Ignored));
        assert_eq!(rec.len(), 1);
        assert_eq!(
            rec.on_key(keys.stop, &keys),
            Some(RecordEvent::Stopped { len: 1 })
        );
    }

    #[test]
    fn ordinary_keys_are_not_record_keys() {
        let keys = toggle_keys();
        let mut rec = Recorder::new();
        assert_eq!(rec.on_key(combo("C-a"), &keys), None);
        // Same VK, different modifiers: F10 replays, S-F10 records, and
        // C-F10 is neither.
        assert_eq!(rec.on_key(combo("C-F10"), &keys), None);
    }

    #[test]
    fn nothing_is_recorded_while_idle() {
        let mut rec = Recorder::new();
        assert_eq!(rec.push(combo("C-a")), None);
        assert!(rec.is_empty());
    }

    #[test]
    fn stop_while_idle_is_swallowed_not_passed_through() {
        let keys = separate_keys();
        let mut rec = Recorder::new();
        // Some(..) is what tells the hook to suppress the event; a stray
        // stop key must not type anything into the foreground app.
        assert_eq!(rec.on_key(keys.stop, &keys), Some(RecordEvent::Ignored));
    }

    #[test]
    fn play_is_ignored_while_recording() {
        let keys = separate_keys();
        let mut rec = Recorder::new();
        rec.on_key(keys.start, &keys);
        assert_eq!(rec.on_key(keys.play, &keys), Some(RecordEvent::Ignored));
        assert!(rec.is_recording());
        assert!(rec.is_empty());
    }

    #[test]
    fn the_limit_truncates_and_ends_the_recording() {
        let keys = toggle_keys();
        let mut rec = Recorder::new();
        rec.on_key(keys.start, &keys);
        for i in 0..MAX_RECORDED_LEN {
            assert_eq!(
                rec.push(combo("C-a")),
                Some(RecordEvent::Recorded { len: i + 1 })
            );
        }
        // The command that would overflow ends it instead of being dropped
        // silently.
        assert_eq!(
            rec.push(combo("C-k")),
            Some(RecordEvent::Truncated {
                len: MAX_RECORDED_LEN
            })
        );
        assert!(!rec.is_recording());
        assert_eq!(rec.recorded().len(), MAX_RECORDED_LEN);
        // Past the limit the recorder is idle, so nothing more accumulates.
        assert_eq!(rec.push(combo("C-y")), None);
        assert_eq!(rec.recorded().len(), MAX_RECORDED_LEN);
    }

    #[test]
    fn starting_again_clears_the_previous_recording() {
        let keys = toggle_keys();
        let mut rec = Recorder::new();
        rec.on_key(keys.start, &keys);
        rec.push(combo("C-a"));
        rec.on_key(keys.stop, &keys);

        rec.on_key(keys.start, &keys);
        assert!(rec.is_empty());
        rec.push(combo("C-y"));
        rec.on_key(keys.stop, &keys);
        assert_eq!(rec.recorded(), [combo("C-y")]);
    }

    #[test]
    fn abort_discards_an_in_progress_recording() {
        let keys = toggle_keys();
        let mut rec = Recorder::new();
        rec.on_key(keys.start, &keys);
        rec.push(combo("C-a"));
        rec.abort();
        assert!(!rec.is_recording());
        assert!(rec.is_empty());
    }
}
