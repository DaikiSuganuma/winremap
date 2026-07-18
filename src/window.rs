//! Foreground-window process name tracking.
//!
//! The keyboard hook must not call Win32 APIs (AGENTS.md invariant 2), so the
//! foreground exe name is queried only when the foreground window changes
//! (`EVENT_SYSTEM_FOREGROUND` via `SetWinEventHook`) and cached. Both the
//! WinEvent callback and the keyboard hook run on the thread that installed
//! them (our main thread's message loop), so a `thread_local` cache needs no
//! synchronization.

use std::cell::RefCell;

use windows::Win32::Foundation::{CloseHandle, HWND, MAX_PATH};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetWindowThreadProcessId, WINEVENT_OUTOFCONTEXT,
};
use windows::core::PWSTR;

thread_local! {
    // Reuses one String so steady-state updates don't grow the heap; the
    // keyboard hook only borrows it read-only.
    static FOREGROUND_EXE: RefCell<String> = RefCell::new(String::with_capacity(64));
}

/// Runs `f` with the cached foreground exe name (lowercase basename, e.g.
/// `"phpstorm64.exe"`; empty when unknown). Hook-safe: no allocation.
pub fn with_foreground_exe<R>(f: impl FnOnce(&str) -> R) -> R {
    FOREGROUND_EXE.with(|cache| f(cache.borrow().as_str()))
}

/// Re-queries the foreground process and updates the cache. Called at startup
/// and from the WinEvent callback — never from the keyboard hook.
pub fn refresh_foreground_cache() {
    // SAFETY: GetForegroundWindow has no preconditions; a null HWND (no
    // foreground window, e.g. during a UAC prompt) is handled below.
    let hwnd = unsafe { GetForegroundWindow() };
    let name = query_exe_basename(hwnd).unwrap_or_default();
    FOREGROUND_EXE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clear();
        cache.push_str(&name);
    });
}

/// Installs the foreground-change watcher on the current thread.
pub fn install_foreground_watch() -> windows::core::Result<HWINEVENTHOOK> {
    // SAFETY: the callback is a static fn kept alive for the process
    // lifetime; WINEVENT_OUTOFCONTEXT means it is dispatched through our own
    // message loop rather than injected into other processes.
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground_changed),
            0, // all processes
            0, // all threads
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if hook.is_invalid() {
        Err(windows::core::Error::from_thread())
    } else {
        Ok(hook)
    }
}

pub fn uninstall_foreground_watch(hook: HWINEVENTHOOK) {
    // SAFETY: called once at shutdown with the handle
    // install_foreground_watch returned.
    let _ = unsafe { UnhookWinEvent(hook) };
}

unsafe extern "system" fn on_foreground_changed(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _time: u32,
) {
    // Query the current foreground window instead of trusting the event's
    // HWND: events can arrive out of order and the latest state is what the
    // next key event will actually be delivered to.
    refresh_foreground_cache();
}

/// Lowercase exe basename for the process owning `hwnd`, or `None` when it
/// cannot be determined (elevated processes deny the query under UIPI; those
/// windows do not receive our injected input anyway, brief §5-5).
fn query_exe_basename(hwnd: HWND) -> Option<String> {
    if hwnd.is_invalid() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: hwnd validity was checked; pid points to a live local.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }

    // SAFETY: pid comes from a live window; the returned handle is closed
    // below on every path.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;

    let mut buf = [0u16; MAX_PATH as usize];
    let mut len = buf.len() as u32;
    // SAFETY: buf outlives the call and len carries its capacity in and the
    // written length out.
    let queried = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    // SAFETY: process is the handle opened above, owned by this function.
    unsafe { CloseHandle(process).ok() };
    queried.ok()?;

    let full = String::from_utf16_lossy(&buf[..len as usize]);
    let basename = full.rsplit(['\\', '/']).next().unwrap_or(&full);
    Some(basename.to_ascii_lowercase())
}
