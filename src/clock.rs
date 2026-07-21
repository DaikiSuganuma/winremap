//! Local wall-clock time for the log's session banners and the settings
//! window's timestamps (ADR 0041).
//!
//! `std::time` only knows the monotonic clock and UTC since the epoch; nothing
//! in std maps that to the user's time zone, and a log stamped in UTC is worse
//! than none when the reader is comparing it against when they pressed a key.
//! Win32 answers it, which is why this file is on the unsafe allowlist
//! (AGENTS.md invariant 3).
//!
//! Nothing here runs on the hook's path.

use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Time::FileTimeToSystemTime;

/// FILETIME counts 100ns ticks from 1601-01-01; the Unix epoch is this many
/// seconds into that.
const EPOCH_DIFFERENCE_SECS: u64 = 11_644_473_600;

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

/// A file's timestamp in the same format, in local time.
///
/// `None` for a time Windows cannot express — a clock far out of range or a
/// corrupt directory entry. The caller shows nothing rather than a wrong date.
pub fn local_from(time: SystemTime) -> Option<String> {
    let unix = time.duration_since(UNIX_EPOCH).ok()?;
    let ticks = unix
        .as_secs()
        .checked_add(EPOCH_DIFFERENCE_SECS)?
        .checked_mul(10_000_000)?
        .checked_add(u64::from(unix.subsec_nanos() / 100))?;
    let utc = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut local = FILETIME::default();
    let mut broken_down = SYSTEMTIME::default();
    // SAFETY: every pointer is to a local that outlives the call, and both
    // calls report a bad conversion through their return value rather than
    // leaving the output half-written.
    unsafe {
        FileTimeToLocalFileTime(&utc, &mut local).ok()?;
        FileTimeToSystemTime(&local, &mut broken_down).ok()?;
    }
    Some(format_time(&broken_down))
}

fn format_time(time: &SYSTEMTIME) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
    )
}
