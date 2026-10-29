//! Tests for `config::last_used` (moved out of the module file, guidelines §5).

use std::path::PathBuf;

use crate::config::last_used::{FILE_NAME, LastUsed, recall, remember};

/// A scratch folder of this test's own; the counter keeps parallel test
/// threads off each other's files without pulling in a temp-file crate.
fn scratch_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("winremap-last-{tag}-{}-{id}", std::process::id()));
    // A previous failed run may have left it behind.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_remembered_file_comes_back() {
    let dir = scratch_dir("roundtrip");
    let config = dir.join("personal-ja.toml");
    std::fs::write(&config, "# mine\n").unwrap();
    let memory = dir.join(FILE_NAME);

    remember(&memory, &config).unwrap();

    assert_eq!(recall(&memory), LastUsed::At(config));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The point of `Gone`: the file was renamed or deleted between runs, and
/// startup falls back to the default *and says so*.
#[test]
fn a_file_that_went_away_is_reported_rather_than_opened() {
    let dir = scratch_dir("gone");
    let config = dir.join("personal-ja.toml");
    std::fs::write(&config, "# mine\n").unwrap();
    let memory = dir.join(FILE_NAME);
    remember(&memory, &config).unwrap();

    std::fs::remove_file(&config).unwrap();

    assert_eq!(recall(&memory), LastUsed::Gone(config));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn no_memory_at_all_is_no_choice() {
    let dir = scratch_dir("absent");
    assert_eq!(recall(&dir.join(FILE_NAME)), LastUsed::Nothing);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Hand-edited or truncated content must not send the next start
/// somewhere unrelated. A relative path is the dangerous one: it would
/// resolve against whatever the working directory happens to be.
#[test]
fn nothing_usable_in_the_file_is_no_choice() {
    let dir = scratch_dir("junk");
    let memory = dir.join(FILE_NAME);
    for content in ["", "\n", "   \n", "config.toml\n", r"..\other.toml"] {
        std::fs::write(&memory, content).unwrap();
        assert_eq!(
            recall(&memory),
            LastUsed::Nothing,
            "{content:?} is not a file to open"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The file is meant to be readable and editable by hand, so the shapes a
/// person or an editor produces have to work: CRLF, and a stray blank
/// line after the path.
#[test]
fn a_hand_edited_memory_still_reads() {
    let dir = scratch_dir("byhand");
    let config = dir.join("keys.toml");
    std::fs::write(&config, "# mine\n").unwrap();
    let memory = dir.join(FILE_NAME);
    std::fs::write(&memory, format!("{}\r\n\r\n", config.display())).unwrap();

    assert_eq!(recall(&memory), LastUsed::At(config));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A rewrite has to replace the whole file, not leave the tail of a
/// longer path behind — the failure `write` avoids and an append would
/// not.
#[test]
fn remembering_again_replaces_the_choice() {
    let dir = scratch_dir("replace");
    let long = dir.join("a-very-long-config-name.toml");
    let short = dir.join("b.toml");
    std::fs::write(&long, "# mine\n").unwrap();
    std::fs::write(&short, "# mine\n").unwrap();
    let memory = dir.join(FILE_NAME);

    remember(&memory, &long).unwrap();
    remember(&memory, &short).unwrap();

    assert_eq!(recall(&memory), LastUsed::At(short));
    std::fs::remove_dir_all(&dir).unwrap();
}
