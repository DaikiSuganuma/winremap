//! The message loop of the thread the hook runs on, and the ways other
//! threads wake it.

use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PostQuitMessage, PostThreadMessageW, TranslateMessage,
    WM_APP,
};

/// Pumps this thread's message queue until WM_QUIT. Both the keyboard hook
/// and the WinEvent hook are serviced by this loop; `on_message` runs after
/// each dispatched message so the tray can drain its event channel without a
/// second thread.
pub fn run_message_loop(mut on_message: impl FnMut()) {
    // SAFETY: no preconditions; returns the calling thread's own id.
    MESSAGE_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
    let mut msg = MSG::default();
    // SAFETY: msg is a live local; a null HWND means "all messages of this
    // thread", which hook dispatch requires. `.0 > 0` also stops on the -1
    // error return, which as_bool() would misread as "keep going".
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        // SAFETY: msg was filled in by the successful GetMessageW above.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        on_message();
    }
}

/// The message loop's thread, so other threads can wake it. Zero until the
/// loop starts.
static MESSAGE_THREAD: AtomicU32 = AtomicU32::new(0);

/// Wakes the message loop from another thread.
///
/// `GetMessageW` blocks, and nothing queues a message for this thread while
/// the user is not typing — so a flag set on the GUI thread (the settings
/// window's reload button) would sit unnoticed until some unrelated event
/// arrived. The message itself carries nothing; waking is the point.
pub fn wake_message_loop() {
    let thread = MESSAGE_THREAD.load(Ordering::SeqCst);
    if thread == 0 {
        return;
    }
    // SAFETY: posting to a thread id is safe even if that thread has exited —
    // the call just fails, and the caller's request is dropped with it.
    let _ = unsafe { PostThreadMessageW(thread, WM_APP, WPARAM(0), LPARAM(0)) };
}

/// Asks the message loop to exit (used by the tray's Quit item).
pub fn post_quit() {
    // SAFETY: no preconditions; posts WM_QUIT to the calling thread's queue.
    unsafe { PostQuitMessage(0) };
}

/// Wakes this thread's own loop from inside the hook callback, after a
/// debug or recording event was queued: keystrokes queue no message for
/// this thread, so `GetMessageW` would sit idle and the ring would only be
/// drained on the next unrelated message (ADR 0016, ADR 0044 — invariant 2's
/// explicit exceptions 1 and 4).
///
/// Non-blocking. A full queue only delays the drain, never the key.
pub(super) fn wake_self() {
    // SAFETY: posts to our own thread's queue; failure (queue full) only
    // delays the drain.
    let _ = unsafe { PostThreadMessageW(GetCurrentThreadId(), WM_APP, WPARAM(0), LPARAM(0)) };
}
