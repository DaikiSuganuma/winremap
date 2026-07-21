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

use std::collections::VecDeque;
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
use winremap::recorder::{MAX_RECORDED_LEN, RecordKeys};

/// Replay the stored macro. wParam carries the side-modifier state observed
/// at the moment the key was pressed.
const MSG_PLAY: u32 = WM_APP + 0x31;
/// Banner commands are waiting in [`BANNER_COMMANDS`].
const MSG_BANNER: u32 = WM_APP + 0x32;
/// The foreground window changed: the banner names the app it is recording
/// and sits on that app's monitor, so both have to follow.
const MSG_FOREGROUND: u32 = WM_APP + 0x33;

/// How long a one-off notice (limit reached, recording cancelled) stays up.
/// Recording progress has no timer: it must not disappear while recording
/// (ADR 0043 decision 3).
const NOTICE_MS: u32 = 2500;

/// What the banner should display next. Queued rather than latest-wins so a
/// "limit reached" notice cannot be swallowed by the hide that follows it.
///
/// `Recording` carries only the count: the app name and the keys are read on
/// the macro-record thread at display time, so they stay right when focus
/// moves mid-recording.
enum BannerCommand {
    Recording { len: usize },
    Notice { text: String },
    Hide,
}

/// Filled on the message loop, drained on the macro-record thread. Never
/// touched from the hook callback, so locking it is fine.
static BANNER_COMMANDS: Mutex<VecDeque<BannerCommand>> = Mutex::new(VecDeque::new());

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

/// The stored macro, for the settings window to display. Never read from the
/// hook callback — this clones.
pub fn recorded() -> Option<Vec<KeyCombo>> {
    RECORDED
        .load()
        .as_ref()
        .map(|commands| (**commands).clone())
}

/// Shows the recording banner, which stays up until told otherwise
/// (ADR 0043 decision 3).
pub fn banner_recording(len: usize) {
    queue_banner(BannerCommand::Recording { len });
}

/// Shows a line that is a notice rather than a state, so it takes itself
/// down again.
pub fn banner_notice(text: String) {
    queue_banner(BannerCommand::Notice { text });
}

pub fn banner_hide() {
    queue_banner(BannerCommand::Hide);
}

fn queue_banner(command: BannerCommand) {
    let tid = THREAD_ID.load(Ordering::Relaxed);
    if tid == 0 {
        return;
    }
    if let Ok(mut queue) = BANNER_COMMANDS.lock() {
        queue.push_back(command);
    }
    banner::post_to_thread(tid, MSG_BANNER, 0);
}

/// Stores a finished recording. Runs on the message loop, outside the hook
/// callback, where allocation is fine.
pub fn publish(commands: &[KeyCombo]) {
    RECORDED.store(Some(std::sync::Arc::new(commands.to_vec())));
}

/// Foreground-change touch point; runs on the main thread's WinEvent
/// callback (not the keyboard hook). One atomic load and one post per app
/// switch while the feature is enabled; the thread drops it unless a
/// recording banner is actually up, which keeps the "is it showing" state
/// in one place instead of mirroring it in an atomic here.
pub fn notify_foreground_changed() {
    let tid = THREAD_ID.load(Ordering::Relaxed);
    if tid != 0 {
        banner::post_to_thread(tid, MSG_FOREGROUND, 0);
    }
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
    let banner = match banner::Banner::create() {
        Ok(banner) => banner,
        Err(e) => {
            crate::notify::error(&i18n::macro_record_failed(&e.to_string()));
            return; // ready is dropped unsent; start_thread sees the Err
        }
    };
    let _ = ready.send(banner::current_thread_id());
    // A panic must stay inside this feature: the remap hook keeps running.
    let outcome = catch_unwind(AssertUnwindSafe(|| run(&banner)));
    THREAD_ID.store(0, Ordering::Release);
    REPLAYING.store(false, Ordering::Relaxed);
    if outcome.is_err() {
        crate::notify::error(&i18n::macro_record_failed("macro-record thread panicked"));
    }
}

fn run(banner: &banner::Banner) {
    // The count while a recording banner is up, so a focus change can
    // redraw it against the new app without the hook resending anything.
    let mut recording: Option<usize> = None;
    while let Some((message, wparam)) = banner::next_thread_message() {
        match message {
            MSG_PLAY => replay(banner, wparam as SideMods),
            MSG_BANNER => {
                while let Some(command) = next_banner_command() {
                    match command {
                        BannerCommand::Recording { len } => {
                            recording = Some(len);
                            show_recording(banner, len);
                        }
                        BannerCommand::Notice { text } => {
                            // A notice replaces the recording state: every
                            // one of them is issued as the recording ends.
                            recording = None;
                            banner.show(&text, Some(NOTICE_MS));
                        }
                        BannerCommand::Hide => {
                            recording = None;
                            banner.hide();
                        }
                    }
                }
            }
            MSG_FOREGROUND => {
                if let Some(len) = recording {
                    show_recording(banner, len);
                }
            }
            _ => {}
        }
    }
}

/// Draws the recording banner against whatever app is in front right now:
/// its name, the keys that end and replay the recording, and the count.
fn show_recording(banner: &banner::Banner, len: usize) {
    let Some(keys) = keys() else {
        return;
    };
    banner.show(
        &i18n::macro_record_banner_recording(
            len,
            MAX_RECORDED_LEN,
            &foreground_app_name(),
            &keys.stop.to_string(),
            &keys.play.to_string(),
        ),
        None,
    );
}

/// The foreground app's exe name, or a placeholder when it cannot be read
/// (an elevated window denies the query under UIPI — and does not receive
/// our input either, brief §5-5).
fn foreground_app_name() -> String {
    crate::window::app_display_name(banner::foreground_window())
        .unwrap_or_else(|| i18n::t().macro_record_unknown_app.to_owned())
}

fn next_banner_command() -> Option<BannerCommand> {
    BANNER_COMMANDS.lock().ok()?.pop_front()
}

fn replay(banner: &banner::Banner, held: SideMods) {
    let Some(commands) = RECORDED.load_full() else {
        return;
    };
    // Set before sending and cleared after, so a play key pressed while the
    // paced replay is still running is ignored by the hook.
    REPLAYING.store(true, Ordering::Relaxed);
    // Painted synchronously by show(), because a paced replay stops this
    // thread from pumping messages for as long as it runs (design doc §6.4).
    banner.show(
        &i18n::macro_record_banner_replaying(&foreground_app_name(), &commands),
        None,
    );
    if crate::hook::debug_enabled() {
        // This thread is not the hook: logging here is fine.
        crate::gui::log::emit(&i18n::macro_record_replaying(&commands));
    }
    crate::sender::send_recorded(&commands, held);
    banner.hide();
    REPLAYING.store(false, Ordering::Relaxed);
}
