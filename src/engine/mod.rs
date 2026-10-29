//! What the hook decides about one key event, as a pure state machine
//! (ADR 0086, v1.1 plan B3).
//!
//! The low-level hook callback used to hold this logic itself, spread over
//! `thread_local`s and interleaved with `SendInput` calls, which made the
//! behaviours that matter most — two-stroke sequences, auto-repeat under a
//! drifting modifier, the Alt/Win menu guard, recording keys taking
//! precedence — the only part of the remapper without a unit test. Here it
//! is a value: feed it [`KeyEvent`]s and read the [`Decision`]s back.
//!
//! The rules the callback lives under still apply, because this runs inside
//! it (AGENTS.md invariant 2): nothing here allocates, locks, or does I/O.
//! The state is fixed-size arrays and `Copy` types, a decision is a `Copy`
//! value with fixed-capacity lists, and a macro's commands are borrowed from
//! the table rather than copied. The binary's `hook` module owns the only
//! [`Engine`], applies each decision — `SendInput`, the debug ring, the
//! recording ring, the IME poke — and nothing else.

#[cfg(test)]
mod tests;

use crate::keymap::sides::{
    ModAdjustment, SideMods, exact_adjustment, side_bit, side_mods_to_mods,
};
use crate::keymap::{KeyCombo, MAX_MACRO_LEN, Mods, Output, RemapTable, Resolution};
use crate::recorder::{RecordEvent, Recorder};

/// Who injected an event the hook observed, read off `dwExtraInfo`
/// (ADR 0006, ADR 0016).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Injected {
    /// WinRemap's own remap output: a substitute key, or a modifier acting
    /// as one. Feeds the logical modifier state so remapped modifiers still
    /// form chords.
    Remap,
    /// WinRemap's transient lift/restore compensation. Deliberately not
    /// tracked: it changes what applications see, not which chord is
    /// logically held.
    Compensation,
    /// Any other software's `SendInput`.
    External,
}

/// One key event as the hook sees it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyEvent {
    pub vk: u16,
    pub down: bool,
    /// `Some` for an injected event (`LLKHF_INJECTED`), saying whose.
    pub injected: Option<Injected>,
}

/// The switches outside the table that change what a key does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Options {
    /// The tray toggle. Only gates *new* remaps: keys already remapped and
    /// held keep their translation until release so no target key gets
    /// stuck down.
    pub enabled: bool,
    /// `--accept-injected` in a `test-inject` build: convert other
    /// software's injections too (ADR 0053). WinRemap's own are passed
    /// through regardless — that is what closes the loop (invariant 1).
    pub accept_injected: bool,
}

/// What the hook decided for one key event, recorded for debug output.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DebugAction {
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
        source: Injected,
    },
}

/// One line of the `--debug` commentary, before it is worded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DebugEvent {
    /// The armed prefix when this was a second stroke.
    pub prev: Option<KeyCombo>,
    pub input: KeyCombo,
    pub action: DebugAction,
}

/// What the sender has to emit for this event. At most one per event.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Emit<'t> {
    /// The press of an exact-rule target, with the modifier surgery it needs.
    ExactDown {
        target: KeyCombo,
        adjustment: ModAdjustment,
    },
    /// Auto-repeat of an already-pressed exact-rule target: only the key.
    ExactRepeat(u16),
    /// The release of an exact-rule target, restoring the modifiers the user
    /// still holds.
    ExactUp {
        target: KeyCombo,
        adjustment: ModAdjustment,
        still_held: SideMods,
    },
    /// Bare-key substitution: the key only, modifiers left alone.
    KeyOnly { vk: u16, up: bool },
    /// A physical Alt/Win release whose chord was consumed: the menu mask
    /// and then the release, as one batch (ADR 0015).
    MaskedModifierUp(u16),
    /// A macro: every chord tapped once, within this press (ADR 0012).
    Sequence(&'t [KeyCombo]),
}

/// Something the recording feature has to be told, outside the callback.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecordNote {
    Event(RecordEvent),
    /// The play key was pressed while idle. Whether a replay can happen is
    /// the macro-record thread's business (is it running, is there anything
    /// recorded, is a replay already under way), so the hook asks it and
    /// turns a refusal into the right [`RecordEvent`].
    Play {
        held: SideMods,
    },
}

/// Debug lines one event can produce: the physical event, and the decision.
pub const MAX_DEBUG_EVENTS: usize = 2;
/// Recording notes one event can produce: one per command of a macro, or
/// one record-key event.
pub const MAX_RECORD_NOTES: usize = MAX_MACRO_LEN + 1;

/// Everything the hook does with one event. Fixed-capacity on purpose: it is
/// built inside the callback.
#[derive(Clone, Copy, Debug)]
pub struct Decision<'t> {
    /// Whether the original event is suppressed (the callback returns
    /// non-zero).
    pub suppress: bool,
    pub emit: Option<Emit<'t>>,
    /// A physical key went down: the IME indicator is told, with the chord,
    /// so it can decide whether the key is a toggle candidate (ADR 0020).
    pub ime_poke: Option<KeyCombo>,
    pub debug: [Option<DebugEvent>; MAX_DEBUG_EVENTS],
    pub records: [Option<RecordNote>; MAX_RECORD_NOTES],
}

impl<'t> Decision<'t> {
    const fn pass() -> Self {
        Self {
            suppress: false,
            emit: None,
            ime_poke: None,
            debug: [None; MAX_DEBUG_EVENTS],
            records: [None; MAX_RECORD_NOTES],
        }
    }

    fn log(&mut self, prev: Option<KeyCombo>, input: KeyCombo, action: DebugAction) {
        if let Some(slot) = self.debug.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(DebugEvent {
                prev,
                input,
                action,
            });
        }
    }

    fn record(&mut self, note: RecordNote) {
        if let Some(slot) = self.records.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(note);
        }
    }

    /// The debug lines, in order.
    pub fn debug_events(&self) -> impl Iterator<Item = &DebugEvent> {
        self.debug.iter().flatten()
    }

    /// The recording notes, in order.
    pub fn record_notes(&self) -> impl Iterator<Item = &RecordNote> {
        self.records.iter().flatten()
    }
}

/// What to do when a suppressed key's physical release (or repeat) arrives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Active {
    /// Chord output: release the target and undo the modifier surgery.
    Exact {
        target: KeyCombo,
        adjustment: ModAdjustment,
    },
    /// Bare-key substitution: release the substitute key.
    KeyOnly { target_vk: u16 },
    /// Nothing to emit — sequence prefixes, macro sources, record keys, and
    /// swallowed unmatched second strokes. Their key-up (and repeat) is
    /// suppressed so applications never see half of a consumed key.
    SuppressUp,
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

/// The hook's state between events. One per hook thread; `Copy`-free but
/// fixed in size, so it can live in a `thread_local` built at compile time.
pub struct Engine {
    /// Logical side-modifier state: physical keys plus modifiers WinRemap
    /// injected as remap output, so CapsLock→LCtrl can still form Ctrl
    /// chords. Compensation events are deliberately not tracked.
    sides: SideMods,
    /// Indexed by original VK. A flat array keeps lookup O(1) without hashing
    /// or allocation inside the callback.
    active: [Option<Active>; 256],
    /// First stroke of a two-stroke sequence, armed until the next
    /// non-modifier key-down consumes it (ADR 0013). No timeout on purpose —
    /// Emacs prefixes wait indefinitely too.
    pending: Option<KeyCombo>,
    /// Alt/Win classes whose chords were consumed while held. Their eventual
    /// physical release must be masked or Windows reads it as a lone tap and
    /// opens the menu bar / Start menu (ADR 0015).
    menu_guard: u8,
    /// Physical down-state per VK, to tell auto-repeats from fresh presses
    /// of keys that are not remapped (their repeats are not debug-logged so
    /// a held key cannot flood the log).
    phys_down: [bool; 256],
    /// The in-progress macro recording (ADR 0043). Fixed-size, so recording
    /// costs the callback a copy and nothing else.
    recorder: Recorder,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub const fn new() -> Self {
        Self {
            sides: 0,
            active: [None; 256],
            pending: None,
            menu_guard: 0,
            phys_down: [false; 256],
            recorder: Recorder::new(),
        }
    }

    /// The side modifiers logically held right now.
    pub fn sides(&self) -> SideMods {
        self.sides
    }

    /// Forgets an armed prefix. The tray's disable does this: a prefix armed
    /// before the toggle must not complete a sequence after it.
    pub fn clear_pending(&mut self) {
        self.pending = None;
    }

    pub fn is_recording(&self) -> bool {
        self.recorder.is_recording()
    }

    /// The commands recorded so far — what the message loop publishes when a
    /// recording ends.
    pub fn recorded(&self) -> &[KeyCombo] {
        self.recorder.recorded()
    }

    /// Drops an in-progress recording; returns whether one was running. The
    /// keys that would end it come from the config, so a reload or the tray's
    /// disable has to abandon it rather than strand it (design doc §5.6).
    pub fn abort_recording(&mut self) -> bool {
        let active = self.recorder.is_recording();
        self.recorder.abort();
        active
    }

    /// Decides one event. `table` is `None` before startup finishes, which
    /// passes everything through; `exe` is the foreground process the rules
    /// are resolved for.
    pub fn on_event<'t>(
        &mut self,
        event: KeyEvent,
        table: Option<&'t RemapTable>,
        exe: &str,
        options: Options,
    ) -> Decision<'t> {
        let mut decision = Decision::pass();
        let KeyEvent { vk, down, injected } = event;

        if let Some(source) = injected {
            // Our own remap output still updates the logical modifier state
            // so remapped modifiers can form chords.
            if source == Injected::Remap
                && let Some(bit) = side_bit(vk)
            {
                self.update_sides(bit, down);
            }
            // Injected events pass through untouched — remapping them could
            // loop (AGENTS.md invariant 1). A `test-inject` build run with
            // `--accept-injected` converts *foreign* injections so the
            // UI-test agent's SendInput reaches the remap path; ours are still
            // passed through unconditionally, and that is what closes the
            // loop (ADR 0053).
            let ours = source != Injected::External;
            if ours || !options.accept_injected {
                // Echo every injected event so the exact delivered stream —
                // including other software's injections — is visible
                // (ADR 0016).
                decision.log(
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
                return decision;
            }
            // Falls through: from here the foreign injection is handled
            // exactly like a physical key, so the debug log shows the remap
            // decision rather than the pass-through echo.
        }

        // A down while already down is an auto-repeat, which the
        // pass-through debug log skips below.
        let physical_repeat = down && self.phys_down[usize::from(vk)];
        self.phys_down[usize::from(vk)] = down;

        // Logged before anything is decided, so the detailed view is a
        // transcript rather than a summary: modifiers, which no decision line
        // ever mentions, and releases, which is where the second half of a
        // remap's output comes from. Auto-repeat is left out — a held key
        // fires dozens of times a second and would bury the rest (ADR 0016's
        // noise rule); the `Repeat` line below already says a key is
        // repeating.
        if !physical_repeat {
            decision.log(
                None,
                KeyCombo {
                    mods: Mods::NONE,
                    vk,
                },
                DebugAction::Physical { up: !down },
            );
        }

        if down {
            // IME indicator poke on toggle-candidate keys; the key always
            // passes through unchanged (invariant 2 exception 3, ADR
            // 0020/0021). The indicator decides whether the key qualifies.
            decision.ime_poke = Some(KeyCombo {
                mods: side_mods_to_mods(self.sides),
                vk,
            });
        }

        if let Some(bit) = side_bit(vk) {
            // Modifier keys are not remappable (config rejects them as
            // inputs), so they only feed the chord state used to match other
            // keys.
            if !down
                && let Some(class) = guard_class(vk)
                && self.take_menu_guard(class)
            {
                // The mask must land before the release, so suppress the
                // physical up and emit [mask, up] as one ordered batch; the
                // injected up carries MARKER_REMAP and updates the sides when
                // it comes back through the hook (ADR 0015).
                decision.emit = Some(Emit::MaskedModifierUp(vk));
                decision.suppress = true;
                return decision;
            }
            self.update_sides(bit, down);
            return decision;
        }

        if down {
            self.on_key_down(vk, physical_repeat, table, exe, options, &mut decision);
        } else {
            self.on_key_up(vk, &mut decision);
        }
        decision
    }

    fn update_sides(&mut self, bit: SideMods, down: bool) {
        self.sides = if down {
            self.sides | bit
        } else {
            self.sides & !bit
        };
    }

    /// Arms the menu guard for the modifier classes involved in a suppressed
    /// chord (see `menu_guard`).
    fn arm_menu_guard(&mut self, mods: Mods) {
        if mods.contains(Mods::ALT) {
            self.menu_guard |= GUARD_ALT;
        }
        if mods.contains(Mods::WIN) {
            self.menu_guard |= GUARD_WIN;
        }
    }

    /// Returns whether `class` was armed, clearing it either way.
    fn take_menu_guard(&mut self, class: u8) -> bool {
        let armed = self.menu_guard & class != 0;
        if armed {
            self.menu_guard &= !class;
        }
        armed
    }

    fn on_key_down<'t>(
        &mut self,
        vk: u16,
        physical_repeat: bool,
        table: Option<&'t RemapTable>,
        exe: &str,
        options: Options,
        decision: &mut Decision<'t>,
    ) {
        // Auto-repeat: keep emitting the target chosen at the initial press
        // even if modifiers drifted since — releasing Ctrl mid-repeat must
        // not morph a remapped C-h back into plain h halfway through. Macro
        // sources and prefixes repeat as nothing: a held key must not spray
        // macros.
        if let Some(kind) = self.active[usize::from(vk)] {
            decision.emit = match kind {
                Active::Exact { target, .. } => Some(Emit::ExactRepeat(target.vk)),
                Active::KeyOnly { target_vk } => Some(Emit::KeyOnly {
                    vk: target_vk,
                    up: false,
                }),
                Active::SuppressUp => None,
            };
            decision.log(
                None,
                KeyCombo {
                    mods: side_mods_to_mods(self.sides),
                    vk,
                },
                DebugAction::Repeat,
            );
            decision.suppress = true;
            return;
        }

        if !options.enabled {
            return;
        }

        let sides = self.sides;
        let input = KeyCombo {
            mods: side_mods_to_mods(sides),
            vk,
        };
        let Some(table) = table else {
            return;
        };

        // Recording keys are taken before any keymap lookup and always
        // suppressed, so they never reach the application (invariant 4). The
        // config rejects a keymap rule on the same key, so nothing is being
        // shadowed silently here (design doc §4/§5.1). An armed prefix is left
        // armed: the next key can still complete the sequence.
        if let Some(keys) = table.macro_record.as_ref()
            && let Some(event) = self.recorder.on_key(input, keys)
        {
            decision.record(match event {
                RecordEvent::Play => RecordNote::Play { held: sides },
                other => RecordNote::Event(other),
            });
            self.arm_menu_guard(input.mods);
            self.active[usize::from(vk)] = Some(Active::SuppressUp);
            decision.suppress = true;
            return;
        }

        if let Some(first) = self.pending.take() {
            // Second stroke of a sequence. Undefined combinations are
            // swallowed (Emacs-style) rather than passed through, so a typo
            // after a prefix cannot leak a stray keystroke into the
            // application.
            let kind = match table.resolve_second(exe, first, input) {
                Some(output) => {
                    decision.log(Some(first), input, debug_action_of(output));
                    self.record_output(output, decision);
                    self.emit_output(output, sides, decision)
                }
                None => {
                    decision.log(Some(first), input, DebugAction::Swallow);
                    Active::SuppressUp
                }
            };
            self.arm_menu_guard(input.mods);
            self.active[usize::from(vk)] = Some(kind);
            decision.suppress = true;
            return;
        }

        let kind = match table.resolve(exe, input) {
            Some(Resolution::Exact(output)) => {
                decision.log(None, input, debug_action_of(output));
                self.record_output(output, decision);
                self.emit_output(output, sides, decision)
            }
            Some(Resolution::KeyOnly(target_vk)) => {
                decision.log(None, input, DebugAction::KeyOnly(target_vk));
                // Bare substitution leaves the physical modifiers alone, so
                // what the application receives is the held modifiers plus
                // the substitute key — record that, not the bare key.
                self.record_command(
                    KeyCombo {
                        mods: input.mods,
                        vk: target_vk,
                    },
                    decision,
                );
                decision.emit = Some(Emit::KeyOnly {
                    vk: target_vk,
                    up: false,
                });
                Active::KeyOnly { target_vk }
            }
            Some(Resolution::Prefix) => {
                decision.log(None, input, DebugAction::Prefix);
                // A prefix emits nothing on its own; the second stroke's
                // output is what gets recorded.
                self.pending = Some(input);
                Active::SuppressUp
            }
            None => {
                // Log a passed key once per physical press: a held key (e.g.
                // a push-to-talk F1) repeats dozens of times per second and
                // would drown the log (ADR 0016's noise rule).
                if !physical_repeat {
                    decision.log(None, input, DebugAction::Pass);
                }
                // A key WinRemap does not remap still reaches the
                // application, so it is part of what the recording has to
                // reproduce. Repeats are skipped for the same reason macros
                // ignore them: a held key must not fill the recording.
                if !physical_repeat {
                    self.record_command(input, decision);
                }
                return;
            }
        };
        // A consumed Alt/Win chord means the eventual physical modifier
        // release must be masked (ADR 0015).
        self.arm_menu_guard(input.mods);
        self.active[usize::from(vk)] = Some(kind);
        decision.suppress = true;
    }

    /// Appends what a key press emitted to the recording, if one is running.
    ///
    /// Recording stores the *remapped output* rather than the key pressed
    /// (ADR 0043 decision 4), which is what lets a replay travel the same
    /// sender path a config macro does. A macro output is stored as its
    /// individual commands, so replaying it is indistinguishable from having
    /// typed them.
    fn record_output(&mut self, output: &Output, decision: &mut Decision<'_>) {
        match output {
            Output::Chord(target) => self.record_command(*target, decision),
            Output::Seq(sequence) => {
                for combo in sequence {
                    self.record_command(*combo, decision);
                }
            }
        }
    }

    fn record_command(&mut self, command: KeyCombo, decision: &mut Decision<'_>) {
        if let Some(event) = self.recorder.push(command) {
            decision.record(RecordNote::Event(event));
        }
    }

    /// Decides the emission for a resolved output and returns the
    /// bookkeeping for its key-up.
    fn emit_output<'t>(
        &mut self,
        output: &'t Output,
        sides: SideMods,
        decision: &mut Decision<'t>,
    ) -> Active {
        match output {
            Output::Chord(target) => {
                let adjustment = exact_adjustment(target.mods, sides);
                decision.emit = Some(Emit::ExactDown {
                    target: *target,
                    adjustment,
                });
                Active::Exact {
                    target: *target,
                    adjustment,
                }
            }
            Output::Seq(sequence) => {
                // Macros complete (downs and ups) within this press; the
                // source key-up has nothing left to do.
                decision.emit = Some(Emit::Sequence(sequence));
                Active::SuppressUp
            }
        }
    }

    fn on_key_up(&mut self, vk: u16, decision: &mut Decision<'_>) {
        let Some(kind) = self.active[usize::from(vk)].take() else {
            // Not a key WinRemap remapped (or its press predated the hook):
            // let the original key-up through so applications never see a
            // stuck key.
            return;
        };
        decision.emit = match kind {
            Active::Exact { target, adjustment } => Some(Emit::ExactUp {
                target,
                adjustment,
                still_held: self.sides,
            }),
            Active::KeyOnly { target_vk } => Some(Emit::KeyOnly {
                vk: target_vk,
                up: true,
            }),
            Active::SuppressUp => None,
        };
        decision.suppress = true;
    }
}

/// The debug line for a resolved output, copied out of the table because a
/// decision must not borrow a `Vec`'s contents past the callback for the
/// log's sake — the log is drained later, on the message loop.
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
