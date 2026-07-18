//! End-to-end tests over the public API: sample configs must always compile
//! and resolve exactly as the docs promise (project brief §9, config-spec §3).

use winremap::config;
use winremap::keymap::{KeyCombo, RemapKind, parse_key_combo};

fn combo(spec: &str) -> KeyCombo {
    parse_key_combo(spec).unwrap()
}

/// The shipped example must keep solving the problem the project started
/// from: PHPStorm-only Ctrl+H → Backspace (project brief §3.1).
#[test]
fn minimal_example_fixes_phpstorm_ctrl_h_only() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/minimal.toml"
    ))
    .unwrap();
    let table = config::parse_str(&source).unwrap();

    let action = table.resolve("phpstorm64.exe", combo("C-h")).unwrap();
    assert_eq!(action.target, combo("Back"));
    assert_eq!(action.kind, RemapKind::Exact);

    // Windows exe names are case-insensitive.
    assert!(table.resolve("PhpStorm64.EXE", combo("C-h")).is_some());

    // Must not leak into other applications or other chords.
    assert!(table.resolve("notepad.exe", combo("C-h")).is_none());
    assert!(table.resolve("phpstorm64.exe", combo("C-S-h")).is_none());
    assert!(table.resolve("phpstorm64.exe", combo("h")).is_none());
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
application = ["phpstorm64.exe"]

[keymap.remap]
"C-h" = "Back"
"#,
    )
    .unwrap();

    let in_app = table.resolve("phpstorm64.exe", combo("C-h")).unwrap();
    assert_eq!(in_app.target, combo("Back"));

    let elsewhere = table.resolve("notepad.exe", combo("C-h")).unwrap();
    assert_eq!(elsewhere.target, combo("Delete"));
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

    let action = table.resolve("notepad.exe", combo("C-h")).unwrap();
    assert_eq!(action.target, combo("Back"));
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

    let chord = table.resolve("x.exe", combo("C-h")).unwrap();
    assert_eq!(chord.kind, RemapKind::Exact);
    assert_eq!(chord.target, combo("Back"));

    // The bare rule still fires for other modifier states.
    let bare = table.resolve("x.exe", combo("A-h")).unwrap();
    assert_eq!(bare.kind, RemapKind::KeyOnly);
    assert_eq!(bare.target, combo("j"));
}
