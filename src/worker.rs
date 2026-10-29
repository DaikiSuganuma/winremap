//! The lifecycle both feature threads share (ADR 0085): the IME indicator
//! and the macro recorder each own one thread with a message loop, started
//! when the config turns the feature on and stopped when it turns it off or
//! WinRemap exits.
//!
//! The contract, which `ime_indicator/mod.rs` and `macro_record/mod.rs` used
//! to spell out twice:
//!
//! * the thread builds its resource (a window) first, and a failure there
//!   ends the thread before it is announced — the caller sees it through
//!   `report` and the id stays 0;
//! * once the resource exists the thread id is published, which is what the
//!   hook reads (wait-free) to decide whether there is anyone to post to;
//! * the body runs under `catch_unwind`, so a panic stays inside the feature
//!   and the hook keeps running (design doc §5, failure isolation);
//! * `stop` posts `WM_QUIT` and joins, and a thread that died on its own was
//!   never joined, so the next `start` joins it first.
//!
//! No `unsafe` here: the thread messages come from `thread_msg`.

use std::fmt::Display;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use crate::thread_msg;

pub struct Worker {
    /// Thread id; 0 while not running. Written by the main thread and the
    /// dying thread itself, read (wait-free) from the hook callback.
    thread_id: AtomicU32,
    /// Kept so `stop` can join. Only touched from the main thread.
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl Worker {
    pub const fn new() -> Self {
        Self {
            thread_id: AtomicU32::new(0),
            handle: Mutex::new(None),
        }
    }

    /// The thread's id, or 0 while it is not running. Hook-safe: one
    /// wait-free load.
    pub fn thread_id(&self) -> u32 {
        self.thread_id.load(Ordering::Relaxed)
    }

    pub fn is_running(&self) -> bool {
        self.thread_id.load(Ordering::Acquire) != 0
    }

    /// Starts the thread. `build` makes the thread's resource on the thread
    /// itself; `run` is the message loop over it; `report` is told, in
    /// English, about a resource that could not be built, a thread that
    /// could not be spawned, or a body that panicked.
    ///
    /// Returns once the thread has either published its id or given up, so
    /// a caller that posts right after this is posting to a live loop.
    pub fn start<T, E: Display>(
        &'static self,
        name: &'static str,
        build: impl FnOnce() -> Result<T, E> + Send + 'static,
        run: impl FnOnce(&T) + Send + 'static,
        report: fn(&str),
    ) {
        let (ready_tx, ready_rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                let resource = match build() {
                    Ok(resource) => resource,
                    Err(e) => {
                        report(&e.to_string());
                        return; // ready is dropped unsent; `start` sees the Err
                    }
                };
                let _ = ready_tx.send(thread_msg::current_thread_id());
                // A panic must stay inside this feature: the remap hook keeps
                // running.
                let outcome = catch_unwind(AssertUnwindSafe(|| run(&resource)));
                self.thread_id.store(0, Ordering::Release);
                if outcome.is_err() {
                    report(&format!("{name} thread panicked"));
                }
            });
        let handle = match spawned {
            Ok(handle) => handle,
            Err(e) => {
                report(&e.to_string());
                return;
            }
        };
        match ready_rx.recv() {
            Ok(tid) => {
                self.thread_id.store(tid, Ordering::Release);
                if let Ok(mut slot) = self.handle.lock()
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

    /// Stops the thread if it runs: posts `WM_QUIT` and joins.
    pub fn stop(&self) {
        let tid = self.thread_id.swap(0, Ordering::AcqRel);
        if tid != 0 {
            thread_msg::post_quit_to(tid);
        }
        if let Ok(mut handle) = self.handle.lock()
            && let Some(handle) = handle.take()
        {
            let _ = handle.join();
        }
    }
}
