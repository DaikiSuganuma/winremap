//! Thread messages for the feature threads — the IME indicator and the
//! macro recorder (ADR 0085): posting to one, pumping one's own queue, and
//! the thread timers the indicator uses to debounce its queries.
//!
//! The two features used to carry identical copies of this in `overlay.rs`
//! and `banner.rs`. One copy here keeps the `unsafe` in one place and lets
//! those files be about their windows.
//!
//! `post_to_thread` is called from inside the hook callback (invariant 2,
//! explicit exceptions 3 and 4): it is one non-blocking Win32 call and never
//! waits. Everything else runs on the feature thread itself.

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, KillTimer, MSG, PostThreadMessageW, SetTimer, TranslateMessage,
    WM_QUIT,
};

pub fn current_thread_id() -> u32 {
    // SAFETY: no preconditions.
    unsafe { GetCurrentThreadId() }
}

/// Non-blocking post; failure (queue full, thread gone) is deliberately
/// ignored — a lost message only means a missed flash or a late banner, and
/// waiting is not an option for a caller inside the hook.
pub fn post_to_thread(tid: u32, message: u32, wparam: usize) {
    // SAFETY: posting to a thread queue is safe for any tid; stale ids fail.
    let _ = unsafe { PostThreadMessageW(tid, message, WPARAM(wparam), LPARAM(0)) };
}

pub fn post_quit_to(tid: u32) {
    post_to_thread(tid, WM_QUIT, 0);
}

/// One-shot thread timer (no window); returns the system-chosen id.
pub fn set_thread_timer(delay_ms: u32) -> usize {
    // SAFETY: no window handle means a plain thread timer delivered to the
    // message loop below.
    unsafe { SetTimer(None, 0, delay_ms, None) }
}

pub fn kill_thread_timer(timer_id: usize) {
    // SAFETY: killing an already-fired timer id is harmless (returns Err).
    let _ = unsafe { KillTimer(None, timer_id) };
}

/// Pumps this thread's queue: window messages (paint, timers) are dispatched
/// to the window procedure, thread messages are returned to the caller as
/// `(message, wParam)`. `None` means WM_QUIT — time to shut down.
pub fn next_thread_message() -> Option<(u32, usize)> {
    let mut msg = MSG::default();
    // SAFETY: msg is a live local; `.0 > 0` also stops on the -1 error
    // return, which as_bool() would misread as "keep going".
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        if msg.hwnd.is_invalid() {
            return Some((msg.message, msg.wParam.0));
        }
        // SAFETY: msg was filled in by the successful GetMessageW above.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    None
}
