//! Local wall-clock time for the log's session banners (ADR 0041).
//!
//! `std::time` only knows the monotonic clock and UTC since the epoch; nothing
//! in std maps that to the user's time zone, and a log stamped in UTC is worse
//! than none when the reader is comparing it against when they pressed a key.
//! One Win32 call answers it, which is why this file is on the unsafe
//! allowlist (AGENTS.md invariant 3).
//!
//! Nothing here runs on the hook's path.

use windows::Win32::System::SystemInformation::GetLocalTime;

/// The current local time as `YYYY-MM-DD HH:MM:SS`.
///
/// Sortable and unambiguous everywhere, which a locale-formatted stamp is not
/// — this ends up in logs users paste into issues.
pub fn local_now() -> String {
    // SAFETY: no arguments to get wrong. GetLocalTime reads the system clock
    // and returns a plain struct; it cannot fail and borrows nothing.
    let now = unsafe { GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, now.wSecond
    )
}
