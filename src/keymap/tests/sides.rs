//! Tests for `keymap::sides` (moved from sender.rs with the arithmetic, v1.1 plan B4).

use crate::keymap::Mods;
use crate::keymap::sides::{SideMods, plan_transition};

const LSHIFT: SideMods = 1;
const LCTRL: SideMods = 1 << 2;
const RCTRL: SideMods = 1 << 3;
const LALT: SideMods = 1 << 4;

#[test]
fn transition_keeps_already_satisfied_modifiers() {
    // The C-t macro case: Ctrl physically held, element needs Ctrl —
    // nothing to release or press (ADR 0017).
    assert_eq!(plan_transition(LCTRL, Mods::CTRL), (0, 0));
    // Either physical side satisfies the class.
    assert_eq!(plan_transition(RCTRL, Mods::CTRL), (0, 0));
}

#[test]
fn transition_adds_only_the_missing_class() {
    // C-Right → C-S-Right: keep Ctrl, add Shift.
    assert_eq!(
        plan_transition(LCTRL, Mods::CTRL.with(Mods::SHIFT)),
        (0, LSHIFT)
    );
}

#[test]
fn transition_swaps_unrelated_modifiers() {
    // A-a's elements: Alt held but the element needs Ctrl only.
    assert_eq!(plan_transition(LALT, Mods::CTRL), (LALT, LCTRL));
}

#[test]
fn transition_releases_everything_for_bare_targets() {
    assert_eq!(
        plan_transition(LCTRL | LSHIFT, Mods::NONE),
        (LCTRL | LSHIFT, 0)
    );
    assert_eq!(plan_transition(0, Mods::NONE), (0, 0));
}
