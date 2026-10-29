//! The invariants of AGENTS.md that a text scan can check, checked (ADR 0083).
//!
//! Three of the seven invariants are about *where* code may live rather than
//! what it does, and until this file existed they were held by review alone:
//! a stray `unsafe` in the wrong module, or a missing `// SAFETY:`, compiled
//! and tested green. These tests read the sources as text — there is no
//! parser to keep in step with the compiler, and the rules are line-shaped
//! anyway.
//!
//! * invariant 3, first half — `unsafe` only in the files listed in
//!   `tests/unsafe-allowlist.txt`, and every listed file still has some;
//! * invariant 3, second half — a `// SAFETY:` comment right above every
//!   `unsafe {` block;
//! * project brief §9 / guidelines §5 — the `winremap` library (the modules
//!   `src/lib.rs` declares) never mentions the `windows` crate, so it keeps
//!   running on a headless CI runner and on no OS at all.
//!
//! Comments are stripped before matching, so prose about `unsafe` is not a
//! violation; a string literal containing `//` loses its tail, which only
//! ever hides a match, never invents one.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const ALLOWLIST: &str = "tests/unsafe-allowlist.txt";

/// Where `unsafe` is looked for. `build.rs` and `tests/` are deliberately
/// left out: neither runs inside the shipped binary, and neither has any.
const SCANNED: &[&str] = &["src", "examples"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Repository-relative path with forward slashes, which is how the allowlist
/// spells them and how a failure message should read on any shell.
fn relative(path: &Path) -> String {
    path.strip_prefix(root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Every `.rs` under `dir`, deepest-first order not mattering: sorted so a
/// failure lists files in a stable order.
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(rust_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
    found.sort();
    found
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", relative(path)))
}

/// The line with any `//` comment removed. Doc comments (`///`, `//!`) go
/// the same way, which is the point: only code counts.
fn code_only(line: &str) -> &str {
    line.find("//").map_or(line, |at| &line[..at])
}

/// Whether the (comment-stripped) line uses `unsafe` in any of its forms.
fn uses_unsafe(code: &str) -> bool {
    ["unsafe {", "unsafe fn", "unsafe extern", "unsafe impl"]
        .iter()
        .any(|form| code.contains(form))
}

fn file_uses_unsafe(path: &Path) -> bool {
    read(path).lines().map(code_only).any(uses_unsafe)
}

/// The allowlist, with its comments and blank lines dropped.
fn allowlist() -> Vec<String> {
    read(&root().join(ALLOWLIST))
        .lines()
        .map(|line| code_only_hash(line).trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

/// The allowlist uses `#` for comments, as a plain text file would.
fn code_only_hash(line: &str) -> &str {
    line.find('#').map_or(line, |at| &line[..at])
}

fn files_using_unsafe() -> BTreeSet<String> {
    SCANNED
        .iter()
        .flat_map(|dir| rust_files(&root().join(dir)))
        .filter(|path| file_uses_unsafe(path))
        .map(|path| relative(&path))
        .collect()
}

/// Invariant 3, first half. Both directions are checked, so the list cannot
/// drift: a file that gained `unsafe` has to be admitted on purpose, and a
/// file that lost its last `unsafe` has to be struck off, or the list would
/// slowly stop meaning anything.
#[test]
fn unsafe_stays_on_the_allowlist() {
    let listed: BTreeSet<String> = allowlist().into_iter().collect();
    let actual = files_using_unsafe();

    let mut problems = Vec::new();
    for path in actual.difference(&listed) {
        problems.push(format!(
            "{path} contains `unsafe` but is not in {ALLOWLIST}"
        ));
    }
    for path in listed.difference(&actual) {
        if root().join(path).is_file() {
            problems.push(format!(
                "{path} is in {ALLOWLIST} but contains no `unsafe` — remove the line"
            ));
        } else {
            problems.push(format!("{path} is in {ALLOWLIST} but does not exist"));
        }
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// Invariant 3, second half: the comment has to sit right above the
/// statement holding the block.
///
/// Walking up from the `unsafe {` line: comment lines are read for the
/// `SAFETY:` tag, attributes are skipped, a blank line ends the search, and
/// at most one line of code is stepped over — that is the `let x =` which
/// rustfmt leaves above a block too long for one line. Measured against the
/// 125 blocks present when this was written, that one line is exactly what
/// the existing code needs and nothing more.
#[test]
fn every_unsafe_block_says_why() {
    let mut problems = Vec::new();
    for path in files_using_unsafe() {
        let text = read(&root().join(&path));
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            if !code_only(line).contains("unsafe {") {
                continue;
            }
            if !has_safety_comment_above(&lines, index) {
                problems.push(format!(
                    "{path}:{}: `unsafe {{` without a `// SAFETY:` comment above it",
                    index + 1
                ));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

fn has_safety_comment_above(lines: &[&str], index: usize) -> bool {
    let mut code_lines_stepped_over = 0;
    for line in lines[..index].iter().rev() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            if trimmed.contains("SAFETY:") {
                return true;
            }
            continue;
        }
        if trimmed.is_empty() {
            return false;
        }
        if trimmed.starts_with("#[") {
            continue;
        }
        code_lines_stepped_over += 1;
        if code_lines_stepped_over > 1 {
            return false;
        }
    }
    false
}

/// The modules `src/lib.rs` declares, as files and directories.
fn library_files() -> Vec<PathBuf> {
    let src = root().join("src");
    let mut files = vec![src.join("lib.rs")];
    for line in read(&src.join("lib.rs")).lines() {
        let code = code_only(line).trim();
        let Some(name) = code
            .strip_prefix("pub mod ")
            .or_else(|| code.strip_prefix("mod "))
            .and_then(|rest| rest.strip_suffix(';'))
        else {
            continue;
        };
        let name = name.trim();
        let dir = src.join(name);
        if dir.is_dir() {
            files.extend(rust_files(&dir));
        } else {
            files.push(src.join(format!("{name}.rs")));
        }
    }
    files
}

/// Project brief §9: the library is pure logic that runs on headless CI. It
/// must not reach for the `windows` crate, nor branch on the platform — a
/// virtual-key code is a `u16` here, not a `VIRTUAL_KEY`.
#[test]
fn the_library_knows_no_windows() {
    const FORBIDDEN: &[&str] = &["windows::", "cfg(windows)", "cfg(target_os"];
    let mut problems = Vec::new();
    for path in library_files() {
        for (index, line) in read(&path).lines().enumerate() {
            let code = code_only(line);
            if let Some(hit) = FORBIDDEN.iter().find(|needle| code.contains(*needle)) {
                problems.push(format!(
                    "{}:{}: `{hit}` in the library crate",
                    relative(&path),
                    index + 1
                ));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// The scan itself has to find what it is looking for, or a regression in
/// `uses_unsafe` would turn every test above green for the wrong reason.
#[test]
fn the_scan_sees_the_hook() {
    assert!(files_using_unsafe().contains("src/hook/mod.rs"));
    assert!(!uses_unsafe(code_only(
        "// this comment mentions unsafe { on purpose"
    )));
    assert!(uses_unsafe(code_only(
        "    let x = unsafe { f() }; // trailing"
    )));
}
