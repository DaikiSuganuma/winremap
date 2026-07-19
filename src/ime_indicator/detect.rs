//! IME open-status query for the foreground window (design doc §3.2).
//!
//! Runs only on the indicator thread — never in the keyboard hook. The
//! cross-process query goes through `SendMessageTimeoutW` so an unresponsive
//! target can stall us at most `QUERY_TIMEOUT_MS`; the plain `SendMessage`
//! is banned (it blocks indefinitely on hung windows).
//!
//! Verified against the modern Microsoft IME on Windows 11 Pro 26200 in plan
//! Phase I1; `examples/ime_probe.rs` stays around as a regression check.

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetClassNameW, GetForegroundWindow, SMTO_ABORTIFHUNG, SendMessageTimeoutW,
    WM_IME_CONTROL,
};
use windows::core::{PCWSTR, w};

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
    /// The foreground window is a shell surface (taskbar, desktop): the
    /// caller ignores the sample entirely (ADR 0023).
    pub shell_surface: bool,
    /// The query went through a UWP CoreWindow child instead of the frame
    /// window (ADR 0023); surfaced in the --debug line for diagnosis.
    pub via_core_window: bool,
}

impl Sample {
    fn none(target: isize) -> Self {
        Self {
            target,
            open: None,
            shell_surface: false,
            via_core_window: false,
        }
    }
}

/// Shell surfaces where an input-mode flash is never wanted: the taskbars,
/// the desktop (`Progman`, or `WorkerW` when wallpaper hosting splits it),
/// the tray-overflow flyouts (Win10 `NotifyIconOverflowWindow`, Win11
/// `TopLevelWindowForOverflowXamlIsland`), and the Win+Space input
/// switcher. Clicking them must not show, hide, or reset the indicator.
fn is_shell_surface(hwnd: HWND) -> bool {
    // 64 units: the longest class matched below is 35 chars and
    // GetClassNameW truncates silently at the buffer size.
    let mut buf = [0u16; 64];
    // SAFETY: hwnd is live (just returned by GetForegroundWindow); buf is a
    // live local and the returned length is bounded by its size.
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len <= 0 {
        return false;
    }
    let class = String::from_utf16_lossy(&buf[..len as usize]);
    matches!(
        class.as_str(),
        "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "Progman"
            | "WorkerW"
            | "NotifyIconOverflowWindow"
            | "TopLevelWindowForOverflowXamlIsland"
            | "Shell_InputSwitchTopLevelWindow"
    )
}

/// Queries whether the IME is on for the current foreground window.
pub fn query_foreground() -> Sample {
    // SAFETY: no preconditions; a null HWND (e.g. during a UAC prompt) is
    // handled below.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return Sample::none(0);
    }
    let target = hwnd.0 as isize;
    if is_shell_surface(hwnd) {
        return Sample {
            shell_surface: true,
            ..Sample::none(target)
        };
    }
    // UWP hosts (ApplicationFrameHost: Settings, Store, ...) put the real
    // input window in a child CoreWindow owned by the app's own process;
    // querying the frame window always reads OFF. Prefer the child when one
    // exists (ADR 0023).
    // SAFETY: hwnd is live; a missing child is the normal Win32 case.
    let input_wnd = unsafe {
        FindWindowExW(
            Some(hwnd),
            None,
            w!("Windows.UI.Core.CoreWindow"),
            PCWSTR::null(),
        )
    }
    .ok()
    .filter(|child| !child.is_invalid());
    let via_core_window = input_wnd.is_some();
    // SAFETY: both handles are live windows; a null result (no IME window)
    // is handled below.
    let ime_wnd = unsafe { ImmGetDefaultIMEWnd(input_wnd.unwrap_or(hwnd)) };
    if ime_wnd.is_invalid() {
        return Sample::none(target);
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
        shell_surface: false,
        via_core_window,
    }
}
