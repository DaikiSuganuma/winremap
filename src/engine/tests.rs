//! The hook's decisions, as a table of key events (ADR 0086). These are the
//! behaviours that used to have no test because they lived inside the
//! callback: two-stroke sequences, auto-repeat under a drifting modifier,
//! the Alt/Win menu guard, recording keys taking precedence, and the
//! pass-through of injected input.

use super::*;
use crate::config;
use crate::keymap::sides::SideMods;
use crate::keymap::tests::us_101;
use crate::keymap::{Layout, parse_key_combo};

const LCTRL: u16 = 0xA2;
const LALT: u16 = 0xA4;
const LSHIFT: u16 = 0xA0;
const VK_H: u16 = 0x48;
const VK_X: u16 = 0x58;
const VK_Q: u16 = 0x51;
const VK_BACK: u16 = 0x08;
const VK_CAPS: u16 = 0x14;
const VK_F10: u16 = 0x79;

const LCTRL_BIT: SideMods = 1 << 2;
const LALT_BIT: SideMods = 1 << 4;

const ON: Options = Options {
    enabled: true,
    accept_injected: false,
};

fn layout() -> Layout {
    us_101()
}

fn combo(spec: &str) -> KeyCombo {
    parse_key_combo(spec, &layout()).unwrap()
}

fn table() -> RemapTable {
    config::parse_str(
        r#"
[macro]
record_start = "S-F10"
record_play = "F10"

[[keymap]]
name = "notepad"
application = ["notepad.exe"]
[keymap.remap]
"C-h" = "Back"
"A-x h" = "C-z"
"C-t" = ["C-Right", "C-Left"]
"CapsLock" = "LCtrl"
"#,
        &layout(),
    )
    .unwrap()
}

fn physical(vk: u16, down: bool) -> KeyEvent {
    KeyEvent {
        vk,
        down,
        injected: None,
    }
}

fn injected(vk: u16, down: bool, source: Injected) -> KeyEvent {
    KeyEvent {
        vk,
        down,
        injected: Some(source),
    }
}

/// Feeds one event to an engine resolving against `notepad.exe`.
fn step<'t>(engine: &mut Engine, table: &'t RemapTable, event: KeyEvent) -> Decision<'t> {
    engine.on_event(event, Some(table), "notepad.exe", ON)
}

fn actions(decision: &Decision<'_>) -> Vec<DebugAction> {
    decision.debug_events().map(|event| event.action).collect()
}

#[test]
fn a_key_without_a_rule_passes_through() {
    let table = table();
    let mut engine = Engine::new();
    let down = step(&mut engine, &table, physical(VK_Q, true));
    assert!(!down.suppress);
    assert_eq!(down.emit, None);
    assert_eq!(
        actions(&down),
        [DebugAction::Physical { up: false }, DebugAction::Pass]
    );
    let up = step(&mut engine, &table, physical(VK_Q, false));
    assert!(!up.suppress);
    assert_eq!(actions(&up), [DebugAction::Physical { up: true }]);
}

/// The founding rule, as a sequence of events: Ctrl held, `h` pressed,
/// released, Ctrl released.
#[test]
fn a_chord_is_substituted_and_its_release_restores_the_modifiers() {
    let table = table();
    let mut engine = Engine::new();

    let ctrl = step(&mut engine, &table, physical(LCTRL, true));
    assert!(!ctrl.suppress, "modifiers always pass through");
    assert_eq!(engine.sides(), LCTRL_BIT);

    let h = step(&mut engine, &table, physical(VK_H, true));
    assert!(h.suppress);
    assert_eq!(
        h.emit,
        Some(Emit::ExactDown {
            target: combo("Back"),
            // Back wants no modifier, so the held Ctrl is lifted around it.
            adjustment: ModAdjustment {
                lifted: LCTRL_BIT,
                added: Mods::NONE,
            },
        })
    );
    assert_eq!(
        actions(&h),
        [
            DebugAction::Physical { up: false },
            DebugAction::Chord(combo("Back"))
        ]
    );

    let h_up = step(&mut engine, &table, physical(VK_H, false));
    assert!(h_up.suppress);
    assert_eq!(
        h_up.emit,
        Some(Emit::ExactUp {
            target: combo("Back"),
            adjustment: ModAdjustment {
                lifted: LCTRL_BIT,
                added: Mods::NONE,
            },
            still_held: LCTRL_BIT,
        })
    );

    let ctrl_up = step(&mut engine, &table, physical(LCTRL, false));
    assert!(!ctrl_up.suppress);
    assert_eq!(engine.sides(), 0);
}

/// Auto-repeat keeps the target chosen at the press even after the modifier
/// is released mid-repeat: a remapped C-h must not turn into plain `h`
/// halfway through.
#[test]
fn a_repeat_keeps_the_target_chosen_at_the_press() {
    let table = table();
    let mut engine = Engine::new();
    step(&mut engine, &table, physical(LCTRL, true));
    step(&mut engine, &table, physical(VK_H, true));
    step(&mut engine, &table, physical(LCTRL, false));

    let repeat = step(&mut engine, &table, physical(VK_H, true));
    assert!(repeat.suppress);
    assert_eq!(repeat.emit, Some(Emit::ExactRepeat(VK_BACK)));
    // A repeat is not a fresh physical press, so only the decision is logged.
    assert_eq!(actions(&repeat), [DebugAction::Repeat]);
}

/// A bare rule substitutes the key and leaves the modifiers alone.
#[test]
fn a_bare_rule_substitutes_the_key_only() {
    let table = table();
    let mut engine = Engine::new();
    let down = step(&mut engine, &table, physical(VK_CAPS, true));
    assert_eq!(
        down.emit,
        Some(Emit::KeyOnly {
            vk: LCTRL,
            up: false
        })
    );
    let up = step(&mut engine, &table, physical(VK_CAPS, false));
    assert_eq!(
        up.emit,
        Some(Emit::KeyOnly {
            vk: LCTRL,
            up: true
        })
    );
    assert!(down.suppress && up.suppress);
}

/// `A-x h`: the prefix emits nothing and waits; the second stroke emits the
/// output; an unbound second stroke is swallowed, Emacs-style.
#[test]
fn a_two_stroke_sequence_waits_for_its_second_stroke() {
    let table = table();
    let mut engine = Engine::new();
    step(&mut engine, &table, physical(LALT, true));
    let prefix = step(&mut engine, &table, physical(VK_X, true));
    assert!(prefix.suppress);
    assert_eq!(prefix.emit, None);
    assert_eq!(
        actions(&prefix),
        [DebugAction::Physical { up: false }, DebugAction::Prefix]
    );
    step(&mut engine, &table, physical(VK_X, false));
    step(&mut engine, &table, physical(LALT, false));
    // The masked release comes back through the hook as an injected up, which
    // is what lowers the side bit (see the menu-guard case).
    step(&mut engine, &table, injected(LALT, false, Injected::Remap));

    let second = step(&mut engine, &table, physical(VK_H, true));
    assert!(second.suppress);
    assert!(matches!(
        second.emit,
        Some(Emit::ExactDown { target, .. }) if target == combo("C-z")
    ));
    let logged = second.debug_events().last().unwrap();
    assert_eq!(logged.prev, Some(combo("A-x")), "the prefix is named");
    assert_eq!(logged.action, DebugAction::Chord(combo("C-z")));

    // Armed again, then an unbound key: swallowed, not passed through.
    step(&mut engine, &table, physical(VK_H, false));
    step(&mut engine, &table, physical(LALT, true));
    step(&mut engine, &table, physical(VK_X, true));
    step(&mut engine, &table, physical(VK_X, false));
    step(&mut engine, &table, physical(LALT, false));
    // The masked release comes back through the hook as an injected up, which
    // is what lowers the side bit (see the menu-guard case).
    step(&mut engine, &table, injected(LALT, false, Injected::Remap));
    let stray = step(&mut engine, &table, physical(VK_Q, true));
    assert!(stray.suppress);
    assert_eq!(stray.emit, None);
    assert_eq!(
        actions(&stray),
        [DebugAction::Physical { up: false }, DebugAction::Swallow]
    );
    let stray_up = step(&mut engine, &table, physical(VK_Q, false));
    assert!(stray_up.suppress, "half of a consumed key never leaks");
}

/// A consumed Alt chord arms the menu guard: the eventual physical Alt
/// release is replaced by [mask, up] so Windows does not read it as a lone
/// tap and open the menu bar (ADR 0015).
#[test]
fn a_consumed_alt_chord_masks_the_alt_release() {
    let table = table();
    let mut engine = Engine::new();
    step(&mut engine, &table, physical(LALT, true));
    step(&mut engine, &table, physical(VK_X, true));
    step(&mut engine, &table, physical(VK_X, false));

    let alt_up = step(&mut engine, &table, physical(LALT, false));
    assert!(alt_up.suppress);
    assert_eq!(alt_up.emit, Some(Emit::MaskedModifierUp(LALT)));
    // The sides follow the injected up when it comes back through the hook,
    // not the suppressed physical one.
    assert_eq!(engine.sides(), LALT_BIT);
    let echo = step(&mut engine, &table, injected(LALT, false, Injected::Remap));
    assert!(!echo.suppress);
    assert_eq!(engine.sides(), 0);

    // Armed once: the next Alt release is an ordinary one.
    step(&mut engine, &table, physical(LALT, true));
    let plain = step(&mut engine, &table, physical(LALT, false));
    assert!(!plain.suppress);
    assert_eq!(plain.emit, None);
}

/// Injected events pass through (invariant 1). WinRemap's own remap output
/// still moves the logical modifier state; compensation does not.
#[test]
fn injected_events_pass_through_and_only_remap_output_moves_the_sides() {
    let table = table();
    let mut engine = Engine::new();
    let remap = step(&mut engine, &table, injected(LCTRL, true, Injected::Remap));
    assert!(!remap.suppress);
    assert_eq!(engine.sides(), LCTRL_BIT);
    assert_eq!(
        actions(&remap),
        [DebugAction::Injected {
            vk: LCTRL,
            up: false,
            source: Injected::Remap
        }]
    );
    let comp = step(
        &mut engine,
        &table,
        injected(LSHIFT, true, Injected::Compensation),
    );
    assert!(!comp.suppress);
    assert_eq!(engine.sides(), LCTRL_BIT, "compensation is not tracked");

    // Another program's C-h is not ours to remap.
    let foreign = step(
        &mut engine,
        &table,
        injected(VK_H, true, Injected::External),
    );
    assert!(!foreign.suppress);
    assert_eq!(foreign.emit, None);
}

/// `--accept-injected` (ADR 0053): foreign injections are remapped like
/// physical keys, WinRemap's own still are not.
#[test]
fn accept_injected_converts_foreign_input_but_never_our_own() {
    let table = table();
    let mut engine = Engine::new();
    let options = Options {
        enabled: true,
        accept_injected: true,
    };
    engine.on_event(physical(LCTRL, true), Some(&table), "notepad.exe", options);
    let foreign = engine.on_event(
        injected(VK_H, true, Injected::External),
        Some(&table),
        "notepad.exe",
        options,
    );
    assert!(foreign.suppress);
    assert!(matches!(foreign.emit, Some(Emit::ExactDown { .. })));
    engine.on_event(physical(VK_H, false), Some(&table), "notepad.exe", options);

    let ours = engine.on_event(
        injected(VK_H, true, Injected::Remap),
        Some(&table),
        "notepad.exe",
        options,
    );
    assert!(!ours.suppress, "our own output is what closes the loop");
}

/// The tray's disable gates new remaps only: a key already remapped and held
/// keeps its translation until release, so no target key gets stuck down.
#[test]
fn disabling_gates_new_remaps_but_not_a_key_already_held() {
    let table = table();
    let mut engine = Engine::new();
    step(&mut engine, &table, physical(LCTRL, true));
    step(&mut engine, &table, physical(VK_H, true));

    let off = Options {
        enabled: false,
        accept_injected: false,
    };
    let repeat = engine.on_event(physical(VK_H, true), Some(&table), "notepad.exe", off);
    assert_eq!(repeat.emit, Some(Emit::ExactRepeat(VK_BACK)));
    let up = engine.on_event(physical(VK_H, false), Some(&table), "notepad.exe", off);
    assert!(matches!(up.emit, Some(Emit::ExactUp { .. })));

    let fresh = engine.on_event(physical(VK_H, true), Some(&table), "notepad.exe", off);
    assert!(!fresh.suppress, "nothing new is remapped while disabled");
    assert_eq!(fresh.emit, None);
}

/// A macro: every chord is emitted within the press, and the source key's
/// release has nothing left to do.
#[test]
fn a_macro_completes_within_the_press() {
    let table = table();
    let mut engine = Engine::new();
    step(&mut engine, &table, physical(LCTRL, true));
    let down = step(&mut engine, &table, physical(0x54, true)); // t
    assert!(down.suppress);
    assert_eq!(
        down.emit,
        Some(Emit::Sequence(&[combo("C-Right"), combo("C-Left")][..]))
    );
    let up = step(&mut engine, &table, physical(0x54, false));
    assert!(up.suppress);
    assert_eq!(up.emit, None);
}

/// The rules of another application do not apply: resolved against the
/// foreground exe the hook names.
#[test]
fn rules_are_resolved_for_the_foreground_application() {
    let table = table();
    let mut engine = Engine::new();
    engine.on_event(physical(LCTRL, true), Some(&table), "chrome.exe", ON);
    let h = engine.on_event(physical(VK_H, true), Some(&table), "chrome.exe", ON);
    assert!(!h.suppress);
    assert_eq!(h.emit, None);
}

/// Before the table exists everything passes through — and the physical
/// bookkeeping still runs, so the first real press is not taken for a repeat.
#[test]
fn without_a_table_everything_passes_through() {
    let mut engine = Engine::new();
    let down = engine.on_event(physical(VK_H, true), None, "notepad.exe", ON);
    assert!(!down.suppress);
    assert_eq!(down.emit, None);
}

/// Every physical key-down pokes the IME indicator with the chord; it is the
/// indicator that decides whether the key is a toggle candidate.
#[test]
fn every_physical_down_pokes_the_ime_indicator() {
    let table = table();
    let mut engine = Engine::new();
    step(&mut engine, &table, physical(LSHIFT, true));
    let down = step(&mut engine, &table, physical(VK_Q, true));
    assert_eq!(down.ime_poke, Some(combo("S-q")));
    let up = step(&mut engine, &table, physical(VK_Q, false));
    assert_eq!(up.ime_poke, None);
    let echo = step(
        &mut engine,
        &table,
        injected(VK_Q, true, Injected::External),
    );
    assert_eq!(echo.ime_poke, None, "injected input is not a keystroke");
}

/// Recording keys come before any rule and are always swallowed; what is
/// recorded is the output a key produced, and the play key is handed to the
/// macro-record feature with the modifiers held at that moment.
#[test]
fn recording_keys_take_precedence_and_record_the_output() {
    let table = table();
    let mut engine = Engine::new();

    step(&mut engine, &table, physical(LSHIFT, true));
    let start = step(&mut engine, &table, physical(VK_F10, true));
    assert!(start.suppress);
    assert_eq!(start.emit, None);
    assert_eq!(
        start.record_notes().copied().collect::<Vec<_>>(),
        [RecordNote::Event(RecordEvent::Started)]
    );
    assert!(engine.is_recording());
    step(&mut engine, &table, physical(VK_F10, false));
    step(&mut engine, &table, physical(LSHIFT, false));

    // A remapped key records its output; a plain key records itself.
    step(&mut engine, &table, physical(LCTRL, true));
    let h = step(&mut engine, &table, physical(VK_H, true));
    assert_eq!(
        h.record_notes().copied().collect::<Vec<_>>(),
        [RecordNote::Event(RecordEvent::Recorded { len: 1 })]
    );
    step(&mut engine, &table, physical(VK_H, false));
    step(&mut engine, &table, physical(LCTRL, false));
    let q = step(&mut engine, &table, physical(VK_Q, true));
    assert_eq!(
        q.record_notes().copied().collect::<Vec<_>>(),
        [RecordNote::Event(RecordEvent::Recorded { len: 2 })]
    );
    step(&mut engine, &table, physical(VK_Q, false));
    assert_eq!(engine.recorded(), [combo("Back"), combo("q")]);

    // The same key stops the recording.
    step(&mut engine, &table, physical(LSHIFT, true));
    let stop = step(&mut engine, &table, physical(VK_F10, true));
    assert_eq!(
        stop.record_notes().copied().collect::<Vec<_>>(),
        [RecordNote::Event(RecordEvent::Stopped { len: 2 })]
    );
    step(&mut engine, &table, physical(VK_F10, false));
    step(&mut engine, &table, physical(LSHIFT, false));

    // Play is handed over for the macro-record thread to answer. The play key
    // is a bare F10, so nothing is held at that moment.
    let play = step(&mut engine, &table, physical(VK_F10, true));
    assert!(play.suppress);
    assert_eq!(
        play.record_notes().copied().collect::<Vec<_>>(),
        [RecordNote::Play { held: 0 }]
    );
}

/// A macro output is recorded as its individual commands, one note each.
#[test]
fn a_macro_output_is_recorded_command_by_command() {
    let table = table();
    let mut engine = Engine::new();
    step(&mut engine, &table, physical(LSHIFT, true));
    step(&mut engine, &table, physical(VK_F10, true));
    step(&mut engine, &table, physical(VK_F10, false));
    step(&mut engine, &table, physical(LSHIFT, false));
    step(&mut engine, &table, physical(LCTRL, true));
    let t = step(&mut engine, &table, physical(0x54, true));
    assert_eq!(
        t.record_notes().copied().collect::<Vec<_>>(),
        [
            RecordNote::Event(RecordEvent::Recorded { len: 1 }),
            RecordNote::Event(RecordEvent::Recorded { len: 2 }),
        ]
    );
}

/// Abandoning a recording (reload, tray disable) says whether there was one.
#[test]
fn aborting_a_recording_reports_whether_one_ran() {
    let table = table();
    let mut engine = Engine::new();
    assert!(!engine.abort_recording());
    step(&mut engine, &table, physical(LSHIFT, true));
    step(&mut engine, &table, physical(VK_F10, true));
    assert!(engine.abort_recording());
    assert!(!engine.is_recording());
}

/// The tray's disable forgets an armed prefix, so a sequence cannot complete
/// across the toggle.
#[test]
fn clearing_the_pending_prefix_drops_an_armed_sequence() {
    let table = table();
    let mut engine = Engine::new();
    step(&mut engine, &table, physical(LALT, true));
    step(&mut engine, &table, physical(VK_X, true));
    step(&mut engine, &table, physical(VK_X, false));
    step(&mut engine, &table, physical(LALT, false));
    // The masked release comes back through the hook as an injected up, which
    // is what lowers the side bit (see the menu-guard case).
    step(&mut engine, &table, injected(LALT, false, Injected::Remap));
    engine.clear_pending();
    let h = step(&mut engine, &table, physical(VK_H, true));
    assert!(!h.suppress, "no prefix is armed any more, so h is just h");
}
