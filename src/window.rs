//! Foreground-window process name tracking.
//!
//! The keyboard hook must not call Win32 APIs (AGENTS.md invariant 2), so the
//! foreground exe name is queried only when the foreground window changes
//! (`EVENT_SYSTEM_FOREGROUND` via `SetWinEventHook`) and cached. Both the
//! WinEvent callback and the keyboard hook run on the thread that installed
//! them (our main thread's message loop), so a `thread_local` cache needs no
//! synchronization.

use std::cell::RefCell;

use crate::gui::log::Kind;
use windows::Win32::Foundation::{CloseHandle, HWND, MAX_PATH};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetWindowThreadProcessId,
    OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
};
use windows::core::PWSTR;

thread_local! {
    // Reuses one String so steady-state updates don't grow the heap; the
    // keyboard hook only borrows it read-only.
    static FOREGROUND_EXE: RefCell<String> = RefCell::new(String::with_capacity(64));
}

/// Runs `f` with the cached foreground exe name (lowercase basename, e.g.
/// `"notepad.exe"`; empty when unknown). Hook-safe: no allocation.
pub fn with_foreground_exe<R>(f: impl FnOnce(&str) -> R) -> R {
    FOREGROUND_EXE.with(|cache| f(cache.borrow().as_str()))
}

/// Asks Windows which window is in front and updates the cache from it.
///
/// **Startup only.** At startup there is no event to learn the window from, so
/// asking is the only option. Everywhere else the answer arrives with the
/// event and must be used instead — see [`on_foreground_changed`], which
/// documents what asking again costs.
pub fn refresh_foreground_cache() {
    // SAFETY: GetForegroundWindow has no preconditions; a null HWND (no
    // foreground window, e.g. during a UAC prompt) is handled below.
    let hwnd = unsafe { GetForegroundWindow() };
    set_foreground_cache(hwnd);
}

/// The foreground application's exe name right now, without touching any
/// cache.
///
/// For the settings window's "capture the foreground app" (B4), which runs on
/// the GUI thread: the cache is a `thread_local`, so refreshing it from there
/// would write a copy the keyboard hook never reads — code that looks like it
/// keeps the hook current and does not. Asking directly is both correct for
/// this caller and honest about what it does; the user has just spent three
/// seconds pointing at a window, so there is no switch in flight to race.
pub fn query_foreground_exe() -> String {
    // SAFETY: GetForegroundWindow has no preconditions; a null HWND is handled
    // by query_exe_path.
    let hwnd = unsafe { GetForegroundWindow() };
    query_exe_path(hwnd)
        .as_deref()
        .map(exe_basename)
        .unwrap_or_default()
}

/// Updates the cache to name `hwnd`'s process. Runs on the main thread — from
/// startup or from the WinEvent callback, never from the keyboard hook — so
/// the allocation and (in debug mode) console output here are fine.
fn set_foreground_cache(hwnd: HWND) {
    let full_path = query_exe_path(hwnd);
    let basename = full_path.as_deref().map(exe_basename).unwrap_or_default();
    FOREGROUND_EXE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clear();
        cache.push_str(&basename);
    });
    if crate::hook::debug_enabled() {
        print_debug_info(full_path.as_deref(), &basename);
    }
}

/// Debug-mode helper for writing config.toml: the exact `application` value
/// to use, which configured keymaps would apply, and the full path.
///
/// Tagged and stamped like the key traffic it sits among (ADR 0058). It used
/// to be three untimed lines led by `[debug]`, which is how a report about
/// the window the user just clicked came to look like part of the previous
/// keystroke (owner request 2026-07-29).
fn print_debug_info(full_path: Option<&str>, basename: &str) {
    let texts = crate::i18n::t();
    let Some(full_path) = full_path else {
        crate::gui::log::tagged(
            Kind::Action,
            texts.log_tag_window,
            texts.debug_foreground_unknown,
        );
        return;
    };
    let table = crate::hook::REMAP_TABLE.load();
    let names: Vec<&str> = table
        .as_ref()
        .map(|t| {
            t.keymaps
                .iter()
                .filter(|k| k.apps.matches(basename))
                .map(|k| k.name.as_str())
                .collect()
        })
        .unwrap_or_default();
    let list = if names.is_empty() {
        texts.debug_none.to_string()
    } else {
        names.join(", ")
    };
    crate::gui::log::tagged(
        Kind::Action,
        texts.log_tag_window,
        &crate::i18n::debug_foreground(basename, &list),
    );
    // The path is the least-read part of the report and the longest, so it
    // goes under the summary, where the detailed view can hide it.
    crate::gui::log::tagged(Kind::Detail, texts.log_tag_window, full_path);
}

/// Lowercase exe basename for an arbitrary window, for display purposes
/// (IME indicator label, ADR 0024). Unlike the cache above this is safe to
/// call from any thread: it only touches locals.
pub fn app_display_name(target: isize) -> Option<String> {
    if target == 0 {
        return None;
    }
    let hwnd = HWND(target as *mut core::ffi::c_void);
    let name = exe_basename(&query_exe_path(hwnd)?);
    (!name.is_empty()).then_some(name)
}

/// Lowercase basename, i.e. the exact string to put in `application`.
fn exe_basename(full_path: &str) -> String {
    full_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(full_path)
        .to_ascii_lowercase()
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

/// The window that came to the foreground, as the event reports it.
///
/// This used to ask `GetForegroundWindow()` instead, on the theory that events
/// can arrive out of order and the current state is what the next key event
/// will be delivered to. **Measured, that theory cost us one switch in five**
/// (ADR 0065): the callback runs while the switch is still settling — 11–18 ms
/// before an independent client watching the same event saw it — and the
/// answer is then the window being *left*. The cache is written with that
/// wrong name, no further event arrives to correct it, and every keystroke
/// until the next switch resolves against the wrong application.
///
/// `hwnd` carries the window the event is about, so there is nothing to race
/// with. Out-of-context events are delivered in order, which is what makes
/// "the last event's window" the current one.
unsafe extern "system" fn on_foreground_changed(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _id_event_thread: u32,
    _time: u32,
) {
    // The window itself, not one of its children: EVENT_SYSTEM_FOREGROUND is
    // documented for OBJID_WINDOW/CHILDID_SELF, and anything else arriving
    // here is not a foreground change.
    if id_object != OBJID_WINDOW.0 || id_child != CHILDID_SELF as i32 {
        return;
    }
    if hwnd.is_invalid() {
        // No window in the event (it can be null while the desktop itself has
        // focus). Asking is all that is left, and there is no switch in
        // progress to be wrong about.
        refresh_foreground_cache();
    } else {
        set_foreground_cache(hwnd);
    }
    // IME indicator touch point: show the panel when focus lands on a window
    // whose IME is on (ADR 0020). No-op unless the feature is enabled.
    crate::ime_indicator::notify_foreground_changed();
    // Macro recording touch point: the banner names the app being recorded
    // and sits on that app's monitor, so both follow the focus.
    crate::macro_record::notify_foreground_changed();
}

/// Full image path for the process owning `hwnd`, or `None` when it cannot
/// be determined (elevated processes deny the query under UIPI; those
/// windows do not receive our injected input anyway, brief §5-5).
fn query_exe_path(hwnd: HWND) -> Option<String> {
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

    Some(String::from_utf16_lossy(&buf[..len as usize]))
}
