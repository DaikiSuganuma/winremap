//! Local wall-clock time for the log's session banners and the settings
//! window's status bar (ADR 0041).
//!
//! `std::time` only knows the monotonic clock and UTC since the epoch; nothing
//! in std maps that to the user's time zone, and a log stamped in UTC is worse
//! than none when the reader is comparing it against when they pressed a key.
//! Win32 answers it, which is why this file is on the unsafe allowlist
//! (AGENTS.md invariant 3).
//!
//! Nothing here runs on the hook's path.

use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::GetLocalTime;

/// The current local time as `YYYY-MM-DD HH:MM:SS`.
///
/// Sortable and unambiguous everywhere, which a locale-formatted stamp is not
/// — this ends up in logs users paste into issues.
pub fn local_now() -> String {
    // SAFETY: no arguments to get wrong. GetLocalTime reads the system clock
    // and returns a plain struct; it cannot fail and borrows nothing.
    let now = unsafe { GetLocalTime() };
    format_time(&now)
}

/// The current local time as `HH:MM:SS.mmm`.
///
/// The log window stamps every line with this. The date is left off because
/// it is the same on every line of a session — the banner at the top carries
/// it — and the column has to stay narrow enough to sit ahead of the text
/// without pushing it off the window.
///
/// The milliseconds are the point rather than a flourish: a remap emits half
/// its keys when you press and the other half when you let go, and at
/// one-second resolution those two moments usually print the same number,
/// which is the reading the stamp exists to prevent (ADR 0057).
pub fn local_time_of_day() -> String {
    // SAFETY: as `local_now` — no arguments, no allocation, cannot fail.
    let now = unsafe { GetLocalTime() };
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        now.wHour, now.wMinute, now.wSecond, now.wMilliseconds
    )
}

fn format_time(time: &SYSTEMTIME) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
    )
}
