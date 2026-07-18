//! End-to-end tests over the public API: sample configs must always compile
//! and resolve exactly as the docs promise (project brief §9, config-spec §3).

use winremap::config;
use winremap::keymap::{KeyCombo, Output, RemapTable, Resolution, parse_key_combo};

fn combo(spec: &str) -> KeyCombo {
    parse_key_combo(spec).unwrap()
}

fn load_example(name: &str) -> RemapTable {
    let path = format!("{}/examples/{name}", env!("CARGO_MANIFEST_DIR"));
    config::parse_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The chord a plain exact rule resolves to, or a panic with context.
fn chord_target(table: &RemapTable, exe: &str, input: &str) -> KeyCombo {
    match table.resolve(exe, combo(input)) {
        Some(Resolution::Exact(Output::Chord(target))) => *target,
        other => panic!("expected chord for {input} in {exe}, got {other:?}"),
    }
}

#[test]
fn minimal_example_fixes_ctrl_h_in_notepad_only() {
    let table = load_example("minimal.toml");

    assert_eq!(chord_target(&table, "notepad.exe", "C-h"), combo("Back"));

    // Windows exe names are case-insensitive.
    assert!(table.resolve("Notepad.EXE", combo("C-h")).is_some());

    // Must not leak into other applications or other chords.
    assert!(table.resolve("explorer.exe", combo("C-h")).is_none());
    assert!(table.resolve("notepad.exe", combo("C-S-h")).is_none());
    assert!(table.resolve("notepad.exe", combo("h")).is_none());
}

/// The scenario the project started from (project brief §1.1/§3.1): an
/// app-scoped C-h → Backspace rule for a JetBrains IDE process.
#[test]
fn per_app_ctrl_h_fix_resolves_for_that_process_only() {
    let table = config::parse_str(
        r#"
[[keymap]]
name = "jetbrains-terminal-fix"
application = ["phpstorm64.exe"]

[keymap.remap]
"C-h" = "Back"
"#,
    )
    .unwrap();

    assert_eq!(chord_target(&table, "phpstorm64.exe", "C-h"), combo("Back"));
    assert!(table.resolve("PhpStorm64.EXE", combo("C-h")).is_some());
    assert!(table.resolve("notepad.exe", combo("C-h")).is_none());
    assert!(table.resolve("phpstorm64.exe", combo("C-S-h")).is_none());
}

/// The Emacs sample must stay parseable and keep its core semantics.
#[test]
fn emacs_example_parses_and_resolves() {
    let table = load_example("emacs.toml");

    let exe = "notepad.exe";
    assert_eq!(chord_target(&table, exe, "C-b"), combo("Left"));
    assert_eq!(chord_target(&table, exe, "C-h"), combo("Back"));
    // Targets with modifiers (word motion -> Ctrl+Arrow).
    assert_eq!(chord_target(&table, exe, "A-f"), combo("C-Right"));
    // Not listed -> untouched.
    assert!(table.resolve("explorer.exe", combo("C-b")).is_none());
}

/// The personal config exercises exclusion lists and macro outputs.
/// Two-stroke sequences are currently commented out in the sample, so they
/// are covered by the unit tests in src/config/tests.rs instead.
#[test]
fn suganuma_example_covers_exclude_and_macros() {
    let table = load_example("suganuma.toml");
    let exe = "notepad.exe";

    // Macro pacing for WinUI apps (ADR 0019) must survive edits.
    assert_eq!(table.macro_delay_ms, 8);

    // Global Emacs bindings apply...
    assert_eq!(chord_target(&table, exe, "C-h"), combo("Back"));
    assert_eq!(chord_target(&table, exe, "A-f"), combo("C-f"));
    // ...but not in excluded apps (not_emacs_target equivalent).
    for excluded in ["Illustrator.exe", "photoshop.exe", "InDesign.exe"] {
        assert!(
            table.resolve(excluded, combo("C-h")).is_none(),
            "{excluded} must be excluded"
        );
    }

    // Macro outputs (select word / open line).
    match table.resolve(exe, combo("C-t")) {
        Some(Resolution::Exact(Output::Seq(seq))) => {
            assert_eq!(seq.len(), 3);
            assert_eq!(seq[0], combo("C-Right"));
            assert_eq!(seq[2], combo("C-S-Right"));
        }
        other => panic!("expected macro for C-t, got {other:?}"),
    }

    // Select-all maps to the native shortcut instead of a macro: injected
    // events are never re-remapped, so emitting C-a cannot collide with the
    // physical C-a -> Home binding.
    assert_eq!(chord_target(&table, exe, "A-a"), combo("C-a"));

    // Browser keymaps override the global rules with identity pass-through.
    assert_eq!(chord_target(&table, "chrome.exe", "C-t"), combo("C-t"));
    assert_eq!(chord_target(&table, "msedge.exe", "C-w"), combo("C-w"));
}

#[test]
fn app_specific_keymap_beats_global_regardless_of_order() {
    // The global section is defined first on purpose: definition order must
    // not decide between specificity classes (ADR 0004).
    let table = config::parse_str(
        r#"
[[keymap]]
name = "global"
application = ["*"]

[keymap.remap]
"C-h" = "Delete"

[[keymap]]
name = "app"
application = ["notepad.exe"]

[keymap.remap]
"C-h" = "Back"
"#,
    )
    .unwrap();

    assert_eq!(chord_target(&table, "notepad.exe", "C-h"), combo("Back"));
    assert_eq!(chord_target(&table, "explorer.exe", "C-h"), combo("Delete"));
}

#[test]
fn first_matching_keymap_wins_within_the_same_class() {
    let table = config::parse_str(
        r#"
[[keymap]]
name = "first"
application = ["*"]

[keymap.remap]
"C-h" = "Back"

[[keymap]]
name = "second"
application = ["*"]

[keymap.remap]
"C-h" = "Delete"
"#,
    )
    .unwrap();

    assert_eq!(chord_target(&table, "notepad.exe", "C-h"), combo("Back"));
}

#[test]
fn exact_rule_beats_bare_rule_in_the_same_keymap() {
    let table = config::parse_str(
        r#"
[[keymap]]
application = ["*"]

[keymap.remap]
"h" = "j"
"C-h" = "Back"
"#,
    )
    .unwrap();

    assert_eq!(chord_target(&table, "x.exe", "C-h"), combo("Back"));

    // The bare rule still fires for other modifier states.
    assert_eq!(
        table.resolve("x.exe", combo("A-h")),
        Some(Resolution::KeyOnly(combo("j").vk))
    );
}
