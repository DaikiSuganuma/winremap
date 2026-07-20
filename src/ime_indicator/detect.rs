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
    EnumChildWindows, GetClassNameW, GetForegroundWindow, SMTO_ABORTIFHUNG, SendMessageTimeoutW,
    WM_IME_CONTROL,
};
use windows::core::BOOL;

/// Bounded wait for the cross-process status query (design doc §3.2).
const QUERY_TIMEOUT_MS: u32 = 100;

/// Not exported by the `windows` crate (it only has IMC_SETOPENSTATUS);
/// value per https://learn.microsoft.com/en-us/windows/win32/intl/wm-ime-control
const IMC_GETOPENSTATUS: usize = 0x0005;

/// Descendants of the foreground window inspected per sample. Windows 11
/// Notepad has ~14; the cap only guards against a pathological window tree.
const MAX_CHILDREN: usize = 128;

/// Distinct IME windows queried per sample — one per GUI thread that owns a
/// window in the foreground app. Ordinary apps have exactly one; the cap
/// bounds the worst case to `MAX_IME_WINDOWS * QUERY_TIMEOUT_MS`.
const MAX_IME_WINDOWS: usize = 4;

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
    /// The answer came from a child window's IME rather than the foreground
    /// window's own (ADR 0033); surfaced in the --debug line for diagnosis.
    pub via_child: bool,
}

impl Sample {
    fn none(target: isize) -> Self {
        Self {
            target,
            open: None,
            shell_surface: false,
            via_child: false,
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

/// The distinct default-IME windows behind a foreground app — one per GUI
/// thread that owns a window in it. A fixed array so the enumeration
/// allocates nothing.
#[derive(Default)]
struct ImeWindows {
    found: [Option<HWND>; MAX_IME_WINDOWS],
    count: usize,
    seen: usize,
}

impl ImeWindows {
    /// Adds the default IME window of `hwnd` unless an earlier window already
    /// contributed it. Returns false when there is no more room, which stops
    /// the enumeration.
    fn add_owner(&mut self, hwnd: HWND) -> bool {
        if self.count >= MAX_IME_WINDOWS {
            return false;
        }
        // SAFETY: hwnd is live — either the foreground window or one handed
        // to us by EnumChildWindows. A null result (no IME for the thread)
        // is the normal case for windows without input.
        let ime = unsafe { ImmGetDefaultIMEWnd(hwnd) };
        if ime.is_invalid() || self.found[..self.count].contains(&Some(ime)) {
            return true;
        }
        self.found[self.count] = Some(ime);
        self.count += 1;
        true
    }

    fn iter(&self) -> impl Iterator<Item = HWND> {
        self.found[..self.count].iter().flatten().copied()
    }
}

/// `EnumChildWindows` callback collecting one IME window per input thread.
unsafe extern "system" fn collect_ime_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the `&mut ImeWindows` passed to EnumChildWindows
    // below, which owns it for the whole (synchronous, single-threaded)
    // enumeration, so this exclusive borrow is never aliased.
    let windows = unsafe { &mut *(lparam.0 as *mut ImeWindows) };
    windows.seen += 1;
    if windows.seen > MAX_CHILDREN {
        return false.into();
    }
    windows.add_owner(hwnd).into()
}

/// Asks one IME window for its open status. `None` on a failed or timed-out
/// query, which the caller treats as "unknown" rather than "off".
fn open_status(ime_wnd: HWND) -> Option<bool> {
    let mut status = 0usize;
    // SAFETY: ime_wnd is a window handle owned by another process; even if
    // it dies mid-call the API just fails. status outlives the call, and
    // SMTO_ABORTIFHUNG plus the timeout bounds the wait.
    let sent = unsafe {
        SendMessageTimeoutW(
            ime_wnd,
            WM_IME_CONTROL,
            WPARAM(IMC_GETOPENSTATUS),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            QUERY_TIMEOUT_MS,
            Some(&raw mut status),
        )
    };
    (sent.0 != 0).then_some(status != 0)
}

/// Queries whether the IME is on for the current foreground window.
///
/// The status lives on the *thread* that owns the focused input surface, and
/// that is not always the thread of the foreground window: UWP hosts put the
/// real input window in a child CoreWindow (ADR 0023), and WinUI 3 apps such
/// as the Windows 11 Notepad run their editor on a second UI thread whose
/// IME window reports ON while the frame's reports OFF (ADR 0033). So every
/// input thread of the app is asked and the first ON wins.
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

    let mut windows = ImeWindows::default();
    // The foreground window's own thread goes first, so an ordinary
    // single-threaded app answers in exactly one query, as it did in v0.1.
    windows.add_owner(hwnd);
    let own_count = windows.count;
    // SAFETY: hwnd is live; the callback only touches `windows`, which
    // outlives this call, and EnumChildWindows returns before it goes away.
    unsafe {
        let _ = EnumChildWindows(
            Some(hwnd),
            Some(collect_ime_window),
            LPARAM(&raw mut windows as isize),
        );
    }

    let mut open = None;
    let mut via_child = false;
    for (index, ime_wnd) in windows.iter().enumerate() {
        let status = open_status(ime_wnd);
        if status == Some(true) {
            return Sample {
                target,
                open: status,
                shell_surface: false,
                via_child: index >= own_count,
            };
        }
        // A definite OFF beats "unknown", whichever thread it came from.
        if open.is_none() && status.is_some() {
            open = status;
            via_child = index >= own_count;
        }
    }
    Sample {
        target,
        open,
        shell_surface: false,
        via_child,
    }
}
