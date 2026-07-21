//! Win32 plumbing for the macro-record thread.
//!
//! Owns everything `unsafe` on that thread (AGENTS.md invariant 3,
//! ADR 0044). The recording/replay banner window itself lands here in
//! milestone M4; for now this is the thread-message plumbing the safe
//! orchestration in mod.rs builds on.

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PostThreadMessageW, TranslateMessage, WM_QUIT,
};

pub fn current_thread_id() -> u32 {
    // SAFETY: no preconditions.
    unsafe { GetCurrentThreadId() }
}

/// Non-blocking post; failure (queue full, thread gone) is deliberately
/// ignored. Called from the hook callback, so it must never wait
/// (invariant 2, explicit exception 4 / ADR 0044).
pub fn post_to_thread(tid: u32, message: u32, wparam: usize) {
    // SAFETY: posting to a thread queue is safe for any tid; stale ids fail.
    let _ = unsafe { PostThreadMessageW(tid, message, WPARAM(wparam), LPARAM(0)) };
}

pub fn post_quit_to(tid: u32) {
    post_to_thread(tid, WM_QUIT, 0);
}

/// Pumps this thread's queue: window messages are dispatched to their window
/// procedure, thread messages are returned to the caller as
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
