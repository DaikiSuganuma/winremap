//! Runtime macro recording: the replay thread and the finished recording
//! (docs/v0.3/02_macro-record-design.md, ADR 0043/0044).
//!
//! Like the IME indicator, this is a self-contained feature kept away from
//! the remapping core: the hook only posts this thread a bounded,
//! non-blocking message (invariant 2, explicit exception 4). The recording
//! itself lives in a `thread_local` [`Recorder`](winremap::recorder::Recorder)
//! on the hook thread; only the *finished* macro crosses over, published
//! here from the message loop where allocating is fine.
//!
//! Replay must never run inside the hook callback: 20 commands at the
//! maximum pacing take 300 ms, which is `LowLevelHooksTimeout`, and Windows
//! would drop the hook (ADR 0044). Deferring to the message loop is not an
//! escape either — it runs on the hook's own thread.
//!
//! This file is orchestration only and stays `unsafe`-free (invariant 3);
//! the Win32 calls live in `banner`.

mod banner;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use arc_swap::ArcSwapOption;
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use crate::i18n;
use crate::sender::SideMods;
use winremap::keymap::KeyCombo;
use winremap::recorder::RecordKeys;

/// Replay the stored macro. wParam carries the side-modifier state observed
/// at the moment the key was pressed.
const MSG_PLAY: u32 = WM_APP + 0x31;

/// Macro-record thread id; 0 while not running. Written by the main thread
/// and the dying thread itself, read (wait-free) from the hook callback.
static THREAD_ID: AtomicU32 = AtomicU32::new(0);
/// Kept so stop() can join. Only touched from the main thread.
static THREAD_HANDLE: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// True while the replay thread is sending. Read wait-free from the hook so
/// a play key pressed mid-replay is ignored rather than starting a second
/// one (owner decision 2026-07-21).
static REPLAYING: AtomicBool = AtomicBool::new(false);

/// The last finished recording, or `None` before anything is recorded.
/// Same wait-free read as the remap table, and published the same way.
static RECORDED: ArcSwapOption<Vec<KeyCombo>> = ArcSwapOption::const_empty();

/// The configured recording keys, or `None` when the feature is off.
/// Hook-safe: a wait-free load and a `Copy` out of the table.
pub fn keys() -> Option<RecordKeys> {
    crate::hook::REMAP_TABLE.load().as_ref()?.macro_record
}

pub fn is_replaying() -> bool {
    REPLAYING.load(Ordering::Relaxed)
}

/// Whether anything has been recorded yet. Read on the message loop to tell
/// "no macro yet" apart from "a replay is already running".
pub fn has_recording() -> bool {
    RECORDED.load().is_some()
}

/// Stores a finished recording. Runs on the message loop, outside the hook
/// callback, where allocation is fine.
pub fn publish(commands: &[KeyCombo]) {
    RECORDED.store(Some(std::sync::Arc::new(commands.to_vec())));
}

/// Hook-callback touch point: asks the macro-record thread to replay.
///
/// Hook-safe by construction: wait-free loads and at most one
/// `PostThreadMessageW` (invariant 2, exception 4 / ADR 0044). Returns
/// whether a replay was requested, so the caller can log why nothing
/// happened.
pub fn request_replay(held: SideMods) -> bool {
    let tid = THREAD_ID.load(Ordering::Relaxed);
    if tid == 0 || is_replaying() || RECORDED.load().is_none() {
        return false;
    }
    banner::post_to_thread(tid, MSG_PLAY, usize::from(held));
    true
}

/// Starts or stops the thread to match the loaded config. Called at startup
/// and after every reload. A config without recording keys costs nothing:
/// no thread, no messages.
pub fn sync_with_config() {
    let running = THREAD_ID.load(Ordering::Acquire) != 0;
    match (keys().is_some(), running) {
        (true, false) => start_thread(),
        // Recording was switched off by an edit: the thread has nothing left
        // to do, and leaving it parked would keep a stale banner alive.
        (false, true) => stop(),
        _ => {}
    }
}

/// Stops the thread if it runs. Called on reload-to-disabled and at
/// shutdown. The recording itself is memory-only and dies with the process
/// (ADR 0043 decision 2).
pub fn stop() {
    let tid = THREAD_ID.swap(0, Ordering::AcqRel);
    if tid != 0 {
        banner::post_quit_to(tid);
    }
    if let Ok(mut handle) = THREAD_HANDLE.lock()
        && let Some(handle) = handle.take()
    {
        let _ = handle.join();
    }
}

fn start_thread() {
    let (ready_tx, ready_rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("macro-record".into())
        .spawn(move || thread_main(&ready_tx));
    let handle = match spawned {
        Ok(handle) => handle,
        Err(e) => {
            crate::notify::error(&i18n::macro_record_failed(&e.to_string()));
            return;
        }
    };
    match ready_rx.recv() {
        Ok(tid) => {
            THREAD_ID.store(tid, Ordering::Release);
            if let Ok(mut slot) = THREAD_HANDLE.lock()
                && let Some(old) = slot.replace(handle)
            {
                // A previous thread that died (panic) was never joined.
                let _ = old.join();
            }
        }
        // The thread reported its own failure before dropping the sender.
        Err(_) => {
            let _ = handle.join();
        }
    }
}

fn thread_main(ready: &mpsc::Sender<u32>) {
    let _ = ready.send(banner::current_thread_id());
    // A panic must stay inside this feature: the remap hook keeps running.
    let outcome = catch_unwind(AssertUnwindSafe(run));
    THREAD_ID.store(0, Ordering::Release);
    REPLAYING.store(false, Ordering::Relaxed);
    if outcome.is_err() {
        crate::notify::error(&i18n::macro_record_failed("macro-record thread panicked"));
    }
}

fn run() {
    while let Some((message, wparam)) = banner::next_thread_message() {
        if message == MSG_PLAY {
            replay(wparam as SideMods);
        }
    }
}

fn replay(held: SideMods) {
    let Some(commands) = RECORDED.load_full() else {
        return;
    };
    // Set before sending and cleared after, so a play key pressed while the
    // paced replay is still running is ignored by the hook.
    REPLAYING.store(true, Ordering::Relaxed);
    if crate::hook::debug_enabled() {
        // This thread is not the hook: logging here is fine.
        crate::gui::log::emit(&i18n::macro_record_replaying(&commands));
    }
    crate::sender::send_recorded(&commands, held);
    REPLAYING.store(false, Ordering::Relaxed);
}
