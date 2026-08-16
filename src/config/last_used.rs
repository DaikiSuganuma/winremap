//! Which config file the next start opens (ADR 0077).
//!
//! The settings window can switch the active config file to any `*.toml`
//! beside it (ADR 0050), and until 1.0.0 that choice lasted exactly as long
//! as the process: the next start silently went back to `config.toml`. A user
//! who keeps their real setup in `personal-ja.toml` had to switch again after
//! every logon, with nothing on screen saying why the keys had changed.
//!
//! So the choice is written down — one line of text holding the absolute path
//! of the file, in a file beside the default config. Deliberately not stored
//! *in* a config file: which config to open cannot be answered by a config
//! that has not been chosen yet, and these files are the user's to edit.
//!
//! Nothing here is a hard failure. A memory that is missing, unreadable,
//! or points at a file that is gone means "no choice was remembered", and the
//! default path applies — the state every run before 1.0.0 was in.

use std::io;
use std::path::{Path, PathBuf};

/// The memory's file name. Beside the config rather than tucked away, so it
/// is visible to a user who opens the settings folder and obvious to delete.
pub const FILE_NAME: &str = "last-config.txt";

/// What the memory says.
#[derive(Debug, PartialEq, Eq)]
pub enum LastUsed {
    /// No choice to honour: no memory file, or nothing usable in it.
    Nothing,
    /// A file was chosen and is not there any more. Kept apart from
    /// [`LastUsed::Nothing`] so startup can say what it went looking for —
    /// a renamed config would otherwise take the keymaps with it and explain
    /// nothing.
    Gone(PathBuf),
    /// The file to open.
    At(PathBuf),
}

/// Reads the memory at `memory`.
pub fn recall(memory: &Path) -> LastUsed {
    let Ok(text) = std::fs::read_to_string(memory) else {
        return LastUsed::Nothing;
    };
    // The first line only, trimmed: an editor may add a trailing newline, and
    // anything past line one was not written by [`remember`].
    let line = text.lines().next().unwrap_or_default().trim();
    if line.is_empty() {
        return LastUsed::Nothing;
    }
    let path = PathBuf::from(line);
    // A relative path would resolve against the working directory, which for
    // a tray app started from a shortcut or the autostart entry is not
    // anywhere the user chose a file.
    if !path.is_absolute() {
        return LastUsed::Nothing;
    }
    if path.is_file() {
        LastUsed::At(path)
    } else {
        LastUsed::Gone(path)
    }
}

/// Records `config` as the file to open next time.
///
/// The path is written as given, so the caller has to hand over the absolute,
/// package-resolved one it is actually using (ADR 0061) — inside an MSIX
/// package the two spellings of `%APPDATA%` name different files, and only
/// one of them exists.
pub fn remember(memory: &Path, config: &Path) -> io::Result<()> {
    let Some(text) = config.to_str() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the config path is not valid UTF-8",
        ));
    };
    if let Some(parent) = memory.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // A trailing newline so the file reads as a line of text in any editor.
    std::fs::write(memory, format!("{text}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder of this test's own; the counter keeps parallel test
    /// threads off each other's files without pulling in a temp-file crate.
    fn scratch_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("winremap-last-{tag}-{}-{id}", std::process::id()));
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
}
