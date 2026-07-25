//! Folder watch behind the address bar's change mark (ADR 0051).
//!
//! `notify` (Windows: `ReadDirectoryChangesW`) watches the config folder
//! while the settings window is showing. Its events are treated purely as a
//! cue: the handler sets a flag and wakes the GUI, and the GUI thread does
//! one `(mtime, size)` comparison to decide what actually changed — editors
//! save via temp files and renames, and interpreting that event soup would
//! tie the mark to their habits. No debouncer crates for the same reason.
//!
//! If the watch cannot start, nothing is lost: `FileList` keeps its 2-second
//! poll as the fallback. The watcher thread (notify's own) touches nothing
//! but the flag and `request_repaint` — never the hook, never a file
//! (invariant 2). No `unsafe` (invariant 3).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use eframe::egui;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// Something in the watched folder changed since the GUI last looked.
static DIRTY: AtomicBool = AtomicBool::new(true);

struct WatchState {
    // Held for its lifetime: dropping it is what stops the watch.
    _watcher: RecommendedWatcher,
    folder: PathBuf,
}

fn slot() -> &'static Mutex<Option<WatchState>> {
    static SLOT: OnceLock<Mutex<Option<WatchState>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Aligns the watch with the settings window: watching its folder while it
/// shows, released when it hides (ADR 0051 decision 2) or the folder moves.
/// Called once per host frame — cheap when nothing changes.
pub fn sync(open: bool, folder: Option<&Path>, ctx: &egui::Context) {
    let Ok(mut state) = slot().lock() else { return };
    match (open, folder) {
        (true, Some(folder)) => {
            if state.as_ref().is_some_and(|watch| watch.folder == folder) {
                return;
            }
            *state = None;
            let waker = ctx.clone();
            let watcher = notify::recommended_watcher(move |_event| {
                // The cue, nothing more: the GUI thread stats and decides.
                DIRTY.store(true, Ordering::SeqCst);
                waker.request_repaint();
            });
            if let Ok(mut watcher) = watcher
                && watcher.watch(folder, RecursiveMode::NonRecursive).is_ok()
            {
                // A fresh watch missed whatever happened before it started.
                DIRTY.store(true, Ordering::SeqCst);
                *state = Some(WatchState {
                    _watcher: watcher,
                    folder: folder.to_owned(),
                });
            }
            // On failure the slot stays None and the poll carries the mark.
        }
        _ => {
            *state = None;
        }
    }
}

/// Whether the event-driven path is live; `false` means poll instead.
pub fn active() -> bool {
    slot().lock().is_ok_and(|state| state.is_some())
}

/// Consumes the cue. One reader — the settings window's `FileList`.
pub fn take_dirty() -> bool {
    DIRTY.swap(false, Ordering::SeqCst)
}
