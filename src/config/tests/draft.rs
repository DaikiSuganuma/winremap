//! Tests for `config::draft` (moved out of the module file, guidelines §5).

use crate::config::draft::*;

const SOURCE: &str = r#"# 全体コメント

[macro]
delay_ms = 8  # ペース

[ime_indicator]
enabled = true
trigger_keys = ["C-Space"]  # 切替

[[keymap]]
name = "emacs"  # Emacs 風
application = [
  "Chrome.exe",  # ブラウザ
  "code.exe",
]

[keymap.remap]
"C-h" = "Back"      # 削除
# 独立コメント

# 密着コメント
"S-C-h" = "Delete"
"C-t" = ["C-Right", "C-Left"]  # マクロ

[[keymap]]
application = ["*"]
exclude = ["game.exe"]

[keymap.remap]
CapsLock = "LCtrl"
"#;

fn draft() -> ConfigDraft {
    parse(SOURCE).expect("fixture parses")
}

#[test]
fn parse_keeps_spellings_and_file_order() {
    let draft = draft();
    assert_eq!(draft.macro_delay, "8");
    assert!(!draft.uses_legacy_delay_key);
    assert_eq!(draft.ime.enabled, Some(true));
    assert_eq!(draft.ime.trigger_keys, "C-Space");
    assert_eq!(draft.keymaps.len(), 2);

    let first = &draft.keymaps[0];
    assert_eq!(first.name, "emacs");
    assert_eq!(first.application, ["Chrome.exe", "code.exe"]);
    let rules: Vec<(&str, &str)> = first
        .rules
        .iter()
        .map(|rule| (rule.input.as_str(), rule.output.as_str()))
        .collect();
    // "S-C-h" stays as written — no canonicalization (ADR 0049), and the
    // file order survives serde-free parsing.
    assert_eq!(
        rules,
        [
            ("C-h", "Back"),
            ("S-C-h", "Delete"),
            ("C-t", "C-Right, C-Left"),
        ]
    );

    let second = &draft.keymaps[1];
    assert_eq!(second.application, ["*"]);
    assert_eq!(second.exclude, ["game.exe"]);
    assert_eq!(second.rules[0].input, "CapsLock");
}

#[test]
fn parse_reads_the_legacy_delay_spelling() {
    let draft = parse("macro_delay_ms = 4  # 旧綴り\n").expect("parses");
    assert!(draft.uses_legacy_delay_key);
    assert_eq!(draft.macro_delay, "4");
}

#[test]
fn unchanged_draft_round_trips_identically() {
    let original = draft();
    let edited = original.clone();
    assert_eq!(apply(SOURCE, &original, &edited).expect("applies"), SOURCE);
}

#[test]
fn output_change_keeps_place_and_comments() {
    let original = draft();
    let mut edited = original.clone();
    edited.keymaps[0].rules[0].output = "Delete".to_owned();
    let saved = apply(SOURCE, &original, &edited).expect("applies");
    assert!(
        saved.contains("\"C-h\" = \"Delete\"      # 削除\n"),
        "{saved}"
    );
    // Everything else untouched.
    assert!(
        saved.contains("# 密着コメント\n\"S-C-h\" = \"Delete\"\n"),
        "{saved}"
    );
    assert!(saved.contains("\"Chrome.exe\",  # ブラウザ"), "{saved}");
}

#[test]
fn added_rule_lands_at_the_table_end() {
    let original = draft();
    let mut edited = original.clone();
    edited.keymaps[0].rules.push(RuleDraft {
        origin: None,
        input: "C-n".to_owned(),
        output: "Down".to_owned(),
    });
    let saved = apply(SOURCE, &original, &edited).expect("applies");
    // Quoted like the rules around it, not bare as toml_edit would write
    // it (owner feedback 2026-07-26).
    assert!(
        saved.contains("\"C-t\" = [\"C-Right\", \"C-Left\"]  # マクロ\n\"C-n\" = \"Down\"\n"),
        "{saved}"
    );
}

#[test]
fn deleted_rule_keeps_the_comment_lines_above_it() {
    let original = draft();
    let mut edited = original.clone();
    // Drop "S-C-h". Both comment blocks above it stand on their own
    // lines, so both move down to the next entry; only the rule's own
    // line goes (ADR 0054).
    edited.keymaps[0].rules.remove(1);
    let saved = apply(SOURCE, &original, &edited).expect("applies");
    assert!(!saved.contains("S-C-h"), "{saved}");
    assert!(
        saved.contains(
            "# 独立コメント\n\n# 密着コメント\n\"C-t\" = [\"C-Right\", \"C-Left\"]  # マクロ"
        ),
        "{saved}"
    );
}

/// The report that changed the rule (owner, acceptance C-4 2026-07-26): a
/// section header sits right on top of the first rule it introduces, so
/// "attached to the entry" was the wrong test for what belongs to it.
#[test]
fn deleting_the_first_rule_of_a_group_keeps_its_header() {
    let source = "[[keymap]]\napplication = [\"*\"]\n\n[keymap.remap]\n\
                  # --- Editing ---\n\"C-h\" = \"Back\"\n\"C-d\" = \"Delete\"\n";
    let original = parse(source).expect("parses");
    let mut edited = original.clone();
    edited.keymaps[0].rules.remove(0);
    let saved = apply(source, &original, &edited).expect("applies");
    assert!(!saved.contains("C-h"), "{saved}");
    assert!(
        saved.contains("# --- Editing ---\n\"C-d\" = \"Delete\""),
        "{saved}"
    );
}

#[test]
fn renamed_input_keeps_place_and_comment() {
    let original = draft();
    let mut edited = original.clone();
    edited.keymaps[0].rules[0].input = "C-y".to_owned();
    let saved = apply(SOURCE, &original, &edited).expect("applies");
    // Same slot, same trailing comment, still quoted.
    assert!(
        saved.contains("\"C-y\" = \"Back\"      # 削除\n# 独立コメント"),
        "{saved}"
    );
    assert!(!saved.contains("\"C-h\""), "{saved}");
}

/// Found in the acceptance run's own fixtures (2026-07-26): a file saved
/// from the editor came back with every line ending changed, so a
/// one-word edit showed up as a whole-file diff.
#[test]
fn saving_keeps_windows_line_endings() {
    let source = SOURCE.replace('\n', "\r\n");
    let original = parse(&source).expect("parses");
    assert_eq!(
        apply(&source, &original, &original.clone()).expect("applies"),
        source,
        "an untouched draft must write the file back byte for byte"
    );

    let mut edited = original.clone();
    edited.keymaps[0].rules[0].output = "Delete".to_owned();
    let saved = apply(&source, &original, &edited).expect("applies");
    assert_eq!(
        saved.matches('\n').count(),
        saved.matches("\r\n").count(),
        "no line may come back with a bare LF: {saved:?}"
    );
}

/// A file that opens with `[[keymap]]` keeps its header comment there —
/// `toml_edit` files that block under the first table, so it used to
/// travel with that keymap (or die with it). Same run as above.
#[test]
fn the_file_header_stays_above_the_first_keymap() {
    let source = "# file header\n# second line\n\n[[keymap]]\nname = \"a\"\n\
                  application = [\"a.exe\"]\n\n[[keymap]]\nname = \"b\"\n\
                  application = [\"b.exe\"]\n";
    let original = parse(source).expect("parses");

    let mut reordered = original.clone();
    reordered.keymaps.swap(0, 1);
    let saved = apply(source, &original, &reordered).expect("applies");
    assert!(
        saved.starts_with("# file header\n# second line\n\n[[keymap]]\nname = \"b\""),
        "{saved}"
    );

    let mut without_first = original.clone();
    without_first.keymaps.remove(0);
    let saved = apply(source, &original, &without_first).expect("applies");
    assert!(
        saved.starts_with("# file header\n# second line\n\n[[keymap]]\nname = \"b\""),
        "{saved}"
    );
}

/// Deleting the *only* keymap left nothing for the header to sit above,
/// and the whole file — comments included — went with it (owner report
/// 2026-07-26, on `examples/minimal.toml`).
#[test]
fn deleting_the_last_keymap_still_keeps_the_file_header() {
    let source = "# file header\n# second line\n\n[[keymap]]\nname = \"a\"\n\
                  application = [\"a.exe\"]\n\n[keymap.remap]\n\"C-h\" = \"Back\"\n";
    let original = parse(source).expect("parses");
    let mut edited = original.clone();
    edited.keymaps.clear();
    let saved = apply(source, &original, &edited).expect("applies");
    assert!(
        saved.starts_with("# file header\n# second line\n"),
        "{saved}"
    );
    assert!(!saved.contains("[[keymap]]"), "{saved}");
}

#[test]
fn reordered_keymaps_move_whole_tables() {
    let original = draft();
    let mut edited = original.clone();
    edited.keymaps.swap(0, 1);
    let saved = apply(SOURCE, &original, &edited).expect("applies");
    let star = saved.find("application = [\"*\"]").expect("global keymap");
    let emacs = saved.find("name = \"emacs\"").expect("named keymap");
    assert!(star < emacs, "{saved}");
    // Each keymap kept its own remap table.
    let caps = saved.find("CapsLock").expect("global rule");
    assert!(caps < emacs, "{saved}");
    // Comments travelled with their lines.
    assert!(
        saved.contains("# 密着コメント\n\"S-C-h\" = \"Delete\""),
        "{saved}"
    );
    // The general sections stayed put at the top.
    assert!(saved.find("[macro]").expect("macro") < star, "{saved}");
}

#[test]
fn added_keymap_appends_a_full_section() {
    let original = draft();
    let mut edited = original.clone();
    edited.keymaps.push(KeymapDraft {
        origin: None,
        name: "notepad".to_owned(),
        application: vec!["notepad.exe".to_owned()],
        exclude: Vec::new(),
        rules: vec![RuleDraft {
            origin: None,
            input: "C-s".to_owned(),
            output: "C-w".to_owned(),
        }],
    });
    let saved = apply(SOURCE, &original, &edited).expect("applies");
    assert!(saved.contains("name = \"notepad\""), "{saved}");
    assert!(saved.contains("application = [\"notepad.exe\"]"), "{saved}");
    assert!(saved.contains("\"C-s\" = \"C-w\""), "{saved}");
    assert!(
        saved.find("notepad").expect("new section") > saved.find("CapsLock").expect("old rule"),
        "{saved}"
    );
}

#[test]
fn deleted_keymap_disappears_with_its_rules() {
    let original = draft();
    let mut edited = original.clone();
    edited.keymaps.remove(0);
    let saved = apply(SOURCE, &original, &edited).expect("applies");
    assert!(!saved.contains("emacs"), "{saved}");
    assert!(!saved.contains("C-h"), "{saved}");
    assert!(saved.contains("CapsLock = \"LCtrl\""), "{saved}");
    assert!(saved.contains("[macro]"), "{saved}");
}

#[test]
fn general_settings_edit_in_place() {
    let original = draft();
    let mut edited = original.clone();
    edited.macro_delay = "4".to_owned();
    edited.ime.enabled = Some(false);
    edited.ime.duration_ms = "1500".to_owned();
    edited.ime.trigger_keys = "C-Space, W-Space".to_owned();
    let saved = apply(SOURCE, &original, &edited).expect("applies");
    assert!(saved.contains("delay_ms = 4  # ペース\n"), "{saved}");
    assert!(saved.contains("enabled = false\n"), "{saved}");
    assert!(saved.contains("duration_ms = 1500"), "{saved}");
    assert!(
        saved.contains("trigger_keys = [\"C-Space\", \"W-Space\"]  # 切替\n"),
        "{saved}"
    );
}

#[test]
fn the_cursor_tint_round_trips_through_the_draft() {
    // The settings window carried neither of these until v0.9, so a config
    // that used them could be opened and saved without the window ever
    // showing them (owner report 2026-08-10). Reading them is what makes
    // them editable; writing them is what makes an edit stick.
    let source = "[ime_indicator]\nchange_cursor_color = true  # 色\ncursor_color = \"#0078d4\"\n";
    let original = parse(source).expect("parses");
    assert_eq!(original.ime.change_cursor_color, Some(true));
    assert_eq!(original.ime.cursor_color, "#0078d4");

    let mut edited = original.clone();
    edited.ime.cursor_color = "#ff8800".to_owned();
    let saved = apply(source, &original, &edited).expect("applies");
    assert!(saved.contains("cursor_color = \"#ff8800\""), "{saved}");
    // The comment on the neighbouring key is untouched, as everywhere else.
    assert!(
        saved.contains("change_cursor_color = true  # 色"),
        "{saved}"
    );
}

#[test]
fn the_cursor_tint_survives_an_edit_that_does_not_touch_it() {
    // What saved v0.8 from losing data: `apply` only writes keys whose
    // draft value changed, so keys the draft did not carry stayed put.
    // Now that the draft does carry them, that has to keep holding.
    let source =
        "[ime_indicator]\nenabled = true\nchange_cursor_color = true\ncursor_color = \"#0078d4\"\n";
    let original = parse(source).expect("parses");
    let mut edited = original.clone();
    edited.ime.enabled = Some(false);
    let saved = apply(source, &original, &edited).expect("applies");
    assert!(saved.contains("change_cursor_color = true"), "{saved}");
    assert!(saved.contains("cursor_color = \"#0078d4\""), "{saved}");
}

#[test]
fn legacy_delay_spelling_is_preserved_on_save() {
    let source = "macro_delay_ms = 4  # 旧綴り\n";
    let original = parse(source).expect("parses");
    let mut edited = original.clone();
    edited.macro_delay = "6".to_owned();
    let saved = apply(source, &original, &edited).expect("applies");
    assert_eq!(saved, "macro_delay_ms = 6  # 旧綴り\n");
}

#[test]
fn cleared_rules_leave_an_empty_remap_table() {
    let original = draft();
    let mut edited = original.clone();
    edited.keymaps[1].rules.clear();
    let saved = apply(SOURCE, &original, &edited).expect("applies");
    assert!(!saved.contains("CapsLock"), "{saved}");
    assert!(saved.contains("[keymap.remap]"), "{saved}");
}

#[test]
fn application_edit_keeps_per_entry_comments() {
    let original = draft();
    let mut edited = original.clone();
    edited.keymaps[0].application.push("firefox.exe".to_owned());
    let saved = apply(SOURCE, &original, &edited).expect("applies");
    assert!(saved.contains("\"Chrome.exe\",  # ブラウザ"), "{saved}");
    assert!(saved.contains("firefox.exe"), "{saved}");
}
