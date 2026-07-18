//! IME open-status query for the foreground window (design doc §3.2).
//!
//! Runs only on the indicator thread — never in the keyboard hook. The
//! cross-process query goes through `SendMessageTimeoutW` so an unresponsive
//! target can stall us at most `QUERY_TIMEOUT_MS`; the plain `SendMessage`
//! is banned (it blocks indefinitely on hung windows).
//!
//! Verified against the modern Microsoft IME on Windows 11 Pro 26200 in plan
//! Phase I1; `examples/ime_probe.rs` stays around as a regression check.

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_IME_CONTROL,
};

/// Bounded wait for the cross-process status query (design doc §3.2).
const QUERY_TIMEOUT_MS: u32 = 100;

/// Not exported by the `windows` crate (it only has IMC_SETOPENSTATUS);
/// value per https://learn.microsoft.com/en-us/windows/win32/intl/wm-ime-control
const IMC_GETOPENSTATUS: usize = 0x0005;

pub struct Sample {
    /// Foreground window handle as an integer (0 = none), used by the caller
    /// both for positioning the overlay and for change detection.
    pub target: isize,
    /// `None` when there is no foreground/IME window or the query failed or
    /// timed out — the caller shows nothing in that case.
    pub open: Option<bool>,
}

/// Queries whether the IME is on for the current foreground window.
pub fn query_foreground() -> Sample {
    // SAFETY: no preconditions; a null HWND (e.g. during a UAC prompt) is
    // handled below.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return Sample {
            target: 0,
            open: None,
        };
    }
    let target = hwnd.0 as isize;
    // SAFETY: hwnd was checked non-null above; a null result (no IME window)
    // is handled below.
    let ime_wnd = unsafe { ImmGetDefaultIMEWnd(hwnd) };
    if ime_wnd.is_invalid() {
        return Sample { target, open: None };
    }
    let mut open_status = 0usize;
    // SAFETY: ime_wnd is a window handle owned by another process; even if
    // it dies mid-call the API just fails. open_status outlives the call,
    // and SMTO_ABORTIFHUNG plus the timeout bounds the wait.
    let sent = unsafe {
        SendMessageTimeoutW(
            ime_wnd,
            WM_IME_CONTROL,
            WPARAM(IMC_GETOPENSTATUS),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            QUERY_TIMEOUT_MS,
            Some(&raw mut open_status),
        )
    };
    Sample {
        target,
        open: (sent.0 != 0).then_some(open_status != 0),
    }
}
