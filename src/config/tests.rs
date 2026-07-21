use super::*;
use crate::keymap::{KeyCombo, Output, Resolution, parse_key_combo};

fn combo(spec: &str) -> KeyCombo {
    parse_key_combo(spec).unwrap()
}

fn issues(source: &str) -> Vec<Issue> {
    match parse_str(source) {
        Err(ConfigError::Invalid(issues)) => issues,
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn compiles_valid_config() {
    let table = parse_str(
        r#"
[[keymap]]
name = "global"
application = ["*"]

[keymap.remap]
"CapsLock" = "LCtrl"

[[keymap]]
application = ["phpstorm64.exe"]

[keymap.remap]
"C-h" = "Back"
"#,
    )
    .unwrap();
    assert_eq!(table.keymaps.len(), 2);
    assert_eq!(table.keymaps[0].bare.len(), 1);
    assert_eq!(table.keymaps[1].exact.len(), 1);
    // Anonymous sections get a positional fallback name for diagnostics.
    assert_eq!(table.keymaps[1].name, "keymap #2");
}

#[test]
fn compiles_exclude_macro_and_sequence_rules() {
    let table = parse_str(
        r#"
[[keymap]]
name = "emacs"
application = ["*"]
exclude = ["Zed.exe"]

[keymap.remap]
"C-t" = ["C-Right", "C-Left", "C-S-Right"]
"A-x u" = "C-z"
"#,
    )
    .unwrap();
    let keymap = &table.keymaps[0];
    assert!(matches!(
        keymap.exact.get(&combo("C-t")),
        Some(Output::Seq(seq)) if seq.len() == 3
    ));
    assert_eq!(
        table.resolve("zed.exe", combo("C-t")),
        None,
        "excluded app must not match"
    );
    assert_eq!(
        table.resolve("notepad.exe", combo("A-x")),
        Some(Resolution::Prefix)
    );
    assert_eq!(
        table.resolve_second("notepad.exe", combo("A-x"), combo("u")),
        Some(&Output::Chord(combo("C-z")))
    );
}

#[test]
fn parses_top_level_macro_delay() {
    let table = parse_str("macro_delay_ms = 8\n[[keymap]]\napplication = [\"*\"]\n").unwrap();
    assert_eq!(table.macro_delay_ms, 8);
    // Absent -> burst mode.
    let table = parse_str("[[keymap]]\napplication = [\"*\"]\n").unwrap();
    assert_eq!(table.macro_delay_ms, 0);
}

#[test]
fn rejects_overlong_macro_delay() {
    let found = issues("macro_delay_ms = 99\n[[keymap]]\napplication = [\"*\"]\n");
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("macro_delay_ms"));
    assert_eq!(found[0].line, 1);
}

#[test]
fn ime_indicator_defaults_when_section_absent() {
    let table = parse_str("[[keymap]]\napplication = [\"*\"]\n").unwrap();
    let settings = table.ime_indicator;
    assert!(!settings.enabled, "feature must be opt-in");
    assert_eq!(settings.duration_ms, 800);
    assert_eq!(settings.size, 96);
    assert_eq!(settings.opacity, 200);
}

#[test]
fn parses_full_ime_indicator_section() {
    let table = parse_str(
        "[ime_indicator]\nenabled = true\nduration_ms = 1500\nsize = 128\nopacity = 255\nshow_app_name = true\n\n[[keymap]]\napplication = [\"*\"]\n",
    )
    .unwrap();
    let settings = table.ime_indicator;
    assert!(settings.enabled);
    assert_eq!(settings.duration_ms, 1500);
    assert_eq!(settings.size, 128);
    assert_eq!(settings.opacity, 255);
    assert!(settings.show_app_name);
    // Opt-in: absent means off.
    let table = parse_str("[[keymap]]\napplication = [\"*\"]\n").unwrap();
    assert!(!table.ime_indicator.show_app_name);
}

#[test]
fn partial_ime_indicator_section_keeps_defaults() {
    let table = parse_str("[ime_indicator]\nenabled = true\n\n[[keymap]]\napplication = [\"*\"]\n")
        .unwrap();
    let settings = table.ime_indicator;
    assert!(settings.enabled);
    assert_eq!(settings.duration_ms, 800);
    assert_eq!(settings.size, 96);
    assert_eq!(settings.opacity, 200);
}

#[test]
fn rejects_out_of_range_ime_indicator_values() {
    let found = issues(
        "[ime_indicator]\nduration_ms = 50\nsize = 999\nopacity = 300\n\n[[keymap]]\napplication = [\"*\"]\n",
    );
    assert_eq!(found.len(), 3, "{found:?}");
    assert!(found[0].message.contains("ime_indicator.duration_ms"));
    assert!(found[0].message.contains("100-5000"));
    assert_eq!(found[0].line, 2);
    assert!(found[1].message.contains("ime_indicator.size"));
    assert_eq!(found[1].line, 3);
    assert!(found[2].message.contains("ime_indicator.opacity"));
    assert_eq!(found[2].line, 4);
}

#[test]
fn parses_ime_indicator_trigger_keys() {
    let table = parse_str(
        "[ime_indicator]\nenabled = true\ntrigger_keys = [\"C-Space\", \"F13\"]\n\n[[keymap]]\napplication = [\"*\"]\n",
    )
    .unwrap();
    assert_eq!(
        table.ime_indicator.trigger_keys,
        vec![combo("C-Space"), combo("F13")]
    );
    // Absent -> empty (built-in VK candidates only).
    let table = parse_str("[[keymap]]\napplication = [\"*\"]\n").unwrap();
    assert!(table.ime_indicator.trigger_keys.is_empty());
}

#[test]
fn rejects_bad_ime_indicator_trigger_keys() {
    let found = issues(
        "[ime_indicator]\ntrigger_keys = [\"C-Bogus\", \"LCtrl\"]\n\n[[keymap]]\napplication = [\"*\"]\n",
    );
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].message.contains("trigger_keys"));
    assert!(found[0].message.contains("Bogus"));
    assert_eq!(found[0].line, 2);
    assert!(found[1].message.contains("cannot be a trigger"));
}

#[test]
fn unknown_ime_indicator_field_is_an_error() {
    let err = parse_str("[ime_indicator]\nenbaled = true\n[[keymap]]\napplication = [\"*\"]\n");
    assert!(err.is_err());
}

#[test]
fn reports_syntax_error_from_toml() {
    let err = parse_str("[[keymap]\n").unwrap_err();
    assert!(matches!(err, ConfigError::Toml(_)));
}

#[test]
fn missing_application_is_an_error() {
    let err = parse_str("[[keymap]]\nname = \"x\"\n").unwrap_err();
    // Reported by serde as a missing field, with toml's span rendering.
    assert!(err.to_string().contains("application"), "{err}");
}

#[test]
fn unknown_field_is_an_error() {
    let err = parse_str("[[keymap]]\napplication = [\"*\"]\napplicatoin = [\"x\"]\n");
    assert!(err.is_err());
}

#[test]
fn reports_bad_key_notation_with_line_numbers() {
    let found = issues(
        "\n[[keymap]]\nname = \"broken\"\napplication = [\"*\"]\n\n[keymap.remap]\n\"C-Bcak\" = \"Back\"\n\"C-h\" = \"Nope\"\n",
    );
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].line, 7);
    assert!(found[0].message.contains("broken"));
    assert!(found[0].message.contains("Bcak"));
    assert_eq!(found[1].line, 8);
    assert!(found[1].message.contains("Nope"));
}

#[test]
fn rejects_wildcard_mixed_with_names() {
    let found = issues("[[keymap]]\napplication = [\"*\", \"notepad.exe\"]\n");
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("wildcard"));
}

#[test]
fn rejects_empty_application_list() {
    let found = issues("[[keymap]]\napplication = []\n");
    assert!(found[0].message.contains("must not be empty"));
}

#[test]
fn rejects_exclude_without_wildcard() {
    let found = issues("[[keymap]]\napplication = [\"notepad.exe\"]\nexclude = [\"zed.exe\"]\n");
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("requires application"));
}

#[test]
fn rejects_modifier_target_on_bare_rule() {
    let found =
        issues("[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"CapsLock\" = \"C-a\"\n");
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("may not have modifiers"));
    assert_eq!(found[0].line, 4);
}

#[test]
fn rejects_macro_target_on_bare_rule() {
    let found = issues(
        "[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"CapsLock\" = [\"a\", \"b\"]\n",
    );
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("may not be a macro"));
}

#[test]
fn rejects_modifier_key_as_input() {
    let found = issues("[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"LCtrl\" = \"a\"\n");
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("cannot be a remap input"));
}

#[test]
fn rejects_plain_and_prefix_conflict() {
    let found = issues(
        "[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"A-x\" = \"F2\"\n\"A-x h\" = \"C-a\"\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    let message = &found[0].message;
    assert!(
        message.contains("sequence prefix"),
        "unexpected message: {message}"
    );
}

#[test]
fn rejects_overlong_macro() {
    let found = issues(
        "[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"C-t\" = [\"a\",\"a\",\"a\",\"a\",\"a\",\"a\",\"a\",\"a\",\"a\"]\n",
    );
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("exceeds"));
}

#[test]
fn rejects_rules_that_normalize_to_the_same_combo() {
    let found = issues(
        "[[keymap]]\napplication = [\"*\"]\n[keymap.remap]\n\"C-h\" = \"Back\"\n\"c-H\" = \"Delete\"\n",
    );
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("duplicates"));
}

#[test]
fn suganuma_example_uses_the_macro_table() {
    let source = include_str!("../../examples/suganuma.toml");
    let table = crate::config::parse_str(source).expect("example config must be valid");
    assert_eq!(table.macro_delay_ms, 8);
    let comments = crate::config::comments::parse(source);
    assert!(comments.general("macro.delay_ms").is_some());
    let first = comments.keymap(0).expect("first keymap");
    assert_eq!(first.exclude("photoshop.exe"), Some("アドビフォトショップ"));
}
