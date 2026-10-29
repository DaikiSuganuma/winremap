//! What this run's GUI and tray share about it, in one place (v1.1 plan D3).
//!
//! Until 1.1 each of these lived in a static of its own — five `OnceLock`
//! slots behind five pairs of accessor functions — and "what state does the
//! GUI carry" had no single answer. The accessors are kept, so callers read
//! as before; what changed is that there is one struct to look at.
//!
//! One lock, held for the length of one read or write and never across a
//! call out. Nothing here is touched from the hook callback (invariant 2):
//! the writers are startup, the tray's reload and the settings window, and
//! the readers are the windows.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use eframe::egui;

use super::log;
use crate::i18n;
use winremap::config::draft::{self, FileStamp};
use winremap::config::last_used;

#[derive(Default)]
struct AppState {
    /// The active config file — the single source of truth (ADR 0050).
    /// Every reader goes through this: the settings window, the editor
    /// launcher, and the tray's reload, so switching the file switches them
    /// all.
    config_path: PathBuf,
    /// Where the choice of config file is remembered between runs
    /// (ADR 0077). `None` when there is nowhere to keep it, which makes a
    /// switch last for this run only — the behaviour of every version before
    /// 1.0.0.
    config_memory: Option<PathBuf>,
    /// The `(mtime, size)` the active config file had when it was last
    /// loaded. The address bar's change mark (●) is "the file no longer
    /// matches this" (v0.4 screen design §2.2).
    loaded_stamp: Option<FileStamp>,
    /// The status bar's message slot: the latest thing that happened,
    /// replaced by the next one, never cleared on its own (v0.4 screen
    /// design §5).
    status: String,
    /// When this process started, formatted once for the status bar.
    started_at: String,
    /// The running loop's context, used to wake it from the tray thread.
    /// eframe paints hidden windows directly (egui Issue #5229), so the host
    /// wakes even while it has nothing on screen.
    wake_context: Option<egui::Context>,
}

fn state() -> MutexGuard<'static, AppState> {
    static STATE: OnceLock<Mutex<AppState>> = OnceLock::new();
    // A poisoned lock still holds the state: a panic on the GUI thread must
    // not take the tray's reload or the status bar down with it.
    STATE
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Records that the config was just loaded. Called for the startup load and
/// for every successful reload, whoever asked for it. Stamps the file and
/// reports to the status bar.
pub fn mark_config_loaded() {
    let stamp = draft::stamp(&active_config_path()).ok();
    state().loaded_stamp = stamp;
    set_status(i18n::t().status_loaded);
}

pub fn loaded_stamp() -> Option<FileStamp> {
    state().loaded_stamp
}

/// Called once from startup; the status bar shows this for the process's
/// whole life.
pub fn mark_started() {
    state().started_at = crate::clock::local_now();
}

pub fn started_at() -> String {
    state().started_at.clone()
}

pub fn set_status(text: &str) {
    state().status = text.to_owned();
}

pub fn status() -> String {
    state().status.clone()
}

/// Records which config file this run uses. Set at startup and by the
/// address bar's file switch (ADR 0050).
pub fn set_config_path(path: PathBuf) {
    state().config_path = path;
}

/// The active config file — the single source of truth (ADR 0050).
pub fn active_config_path() -> PathBuf {
    state().config_path.clone()
}

/// Called once from startup, before any window can be opened.
pub fn set_config_memory(path: Option<PathBuf>) {
    state().config_memory = path;
}

/// The address bar's file switch: the file becomes the active one **and** the
/// one the next start opens (ADR 0077).
///
/// Startup does not come through here, on purpose. Choosing a file is
/// something the user does in this window; a `--config` on the command line
/// is an instruction for one run, and must not overwrite the choice — the
/// acceptance probe alone starts WinRemap on a throwaway config half a dozen
/// times per run.
pub(crate) fn choose_config_path(path: PathBuf) {
    let memory = state().config_memory.clone();
    if let Some(memory) = memory
        && let Err(e) = last_used::remember(&memory, &path)
    {
        // Said out loud rather than swallowed: what goes wrong here is
        // invisible until the *next* start opens the old file, which is the
        // confusion this feature exists to remove.
        log::emit(&i18n::remember_config_failed(&e.to_string()));
    }
    set_config_path(path);
}

/// The loop's context once it runs, `None` once it is gone.
pub(super) fn set_wake_context(ctx: Option<egui::Context>) {
    state().wake_context = ctx;
}

/// Asks the running loop for a frame, so a pending open is handled now
/// rather than at the next poll. Nothing happens when no loop runs.
pub(super) fn wake_gui() {
    let ctx = state().wake_context.clone();
    if let Some(ctx) = ctx {
        ctx.request_repaint();
    }
}
