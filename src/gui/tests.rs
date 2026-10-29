//! Tests for the GUI's shared state (moved out of the module file, guidelines §5).

use super::*;

/// The wiring under the address bar's file switch (ADR 0077 decision 1):
/// the file becomes the active one **and** the next start's.
///
/// The click itself belongs to the UI checks; what this pins is that the
/// switch goes through the path that writes the choice down. Until 1.0.0
/// it called `set_config_path`, which does only the first half — and the
/// difference is invisible until the *next* start opens the old file.
#[test]
fn choosing_a_file_records_it_for_the_next_start() {
    use winremap::config::last_used::{FILE_NAME, LastUsed, recall};

    let dir = std::env::temp_dir().join(format!("winremap-choose-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let memory = dir.join(FILE_NAME);
    let chosen = dir.join("personal-ja.toml");
    std::fs::write(&chosen, "# mine\n").unwrap();
    set_config_memory(Some(memory.clone()));

    choose_config_path(chosen.clone());

    assert_eq!(active_config_path(), chosen, "the switch takes effect now");
    assert_eq!(
        recall(&memory),
        LastUsed::At(chosen),
        "and again at the next start"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
