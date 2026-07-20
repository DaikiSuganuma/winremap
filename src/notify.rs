//! Telling the user something outside the tray: the parent terminal's console
//! and modal dialogs.
//!
//! WinRemap is a `windows` subsystem binary (ADR 0029), so it never opens a
//! console window of its own. Output only reaches a terminal when the user
//! started it from one, and anything the user *must* see has to become a
//! dialog when there is no terminal. Both halves of that live here so the
//! `unsafe` stays out of main.rs, by the same reasoning as ADR 0009
//! (AGENTS.md invariant 3, ADR 0031).

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE,
    STD_OUTPUT_HANDLE, SetStdHandle,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MESSAGEBOX_STYLE, MessageBoxW,
};
use windows::core::{HSTRING, PCWSTR, w};

/// Whether stdout/stderr reach a terminal. Read from the message loop and the
/// tray callback, so it is an atomic rather than a `OnceLock<bool>`.
static HAS_CONSOLE: AtomicBool = AtomicBool::new(false);

/// Attaches to the console of the process that launched us, if it has one.
///
/// Call once, before any output. Returns whether printing now goes anywhere:
/// true when started from a terminal (`winremap --debug`) or with stdout
/// redirected, false for Explorer, the Start menu, and the sign-in autostart
/// entry — which is the point, since those must not flash a console window
/// (ADR 0029).
pub fn attach_parent_console() -> bool {
    // Checked before attaching: a handle the launcher already set means we
    // were redirected (`winremap --help > out.txt`, or a pipe), and that
    // must win over both the console and the dialog fallback — otherwise a
    // script capturing our output would block on a message box instead.
    let redirected = std_handle_is_set(STD_OUTPUT_HANDLE);
    // SAFETY: no arguments to get wrong; failure just means the launcher had
    // no console (Explorer, autostart), which is the expected silent case.
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_ok();
    if attached {
        // Attaching does not fill in handles the subsystem left unset, and
        // adopt_console_handle leaves already-set ones alone.
        adopt_console_handle(STD_OUTPUT_HANDLE);
        adopt_console_handle(STD_ERROR_HANDLE);
    }
    let reachable = attached || redirected;
    HAS_CONSOLE.store(reachable, Ordering::Relaxed);
    reachable
}

/// Whether the launcher handed us this standard handle (console, pipe, file).
fn std_handle_is_set(which: STD_HANDLE) -> bool {
    // SAFETY: `which` is one of the documented STD_* constants.
    matches!(unsafe { GetStdHandle(which) }, Ok(handle) if !handle.is_invalid())
}

/// True when `println!`/`eprintln!` actually reach the user.
pub fn has_console() -> bool {
    HAS_CONSOLE.load(Ordering::Relaxed)
}

/// Points one standard handle at the attached console, but only if the
/// subsystem left it unset — an already-set handle means the caller
/// redirected us to a file or pipe and expects that to win.
fn adopt_console_handle(which: STD_HANDLE) {
    if std_handle_is_set(which) {
        return;
    }
    // SAFETY: CONOUT$ is the console's screen buffer; the handle is handed to
    // SetStdHandle, which takes ownership for the life of the process.
    let console = unsafe {
        CreateFileW(
            w!("CONOUT$"),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };
    let Ok(console) = console else { return };
    if console.is_invalid() {
        return;
    }
    // SAFETY: `console` is a valid handle to the console we just attached to.
    let _ = unsafe { SetStdHandle(which, console) };
}

/// Shows a message the user must not miss. Prefers the console so terminal
/// users keep a copyable transcript; falls back to a dialog when there is
/// none, so a silent launch never swallows the message (ADR 0029).
pub fn error(message: &str) {
    // The log window shows it too when open: a dialog has to be dismissed,
    // and the reason for a failed reload is worth keeping around.
    crate::log_window::push(message);
    if has_console() {
        eprintln!("{message}");
    } else {
        message_box(message, MB_ICONERROR);
    }
}

/// Same routing as [`error`] for output that is informational rather than a
/// failure — `--help` and `--version` when launched without a terminal.
pub fn info(message: &str) {
    if has_console() {
        println!("{message}");
    } else {
        message_box(message, MB_ICONINFORMATION);
    }
}

fn message_box(message: &str, icon: MESSAGEBOX_STYLE) {
    let text = HSTRING::from(message);
    let caption = HSTRING::from(crate::i18n::t().app_name);
    // SAFETY: both strings outlive the call; a null owner window is valid and
    // gives a top-level dialog, which is what a tray app wants.
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_OK | icon | MB_SETFOREGROUND,
        );
    }
}

/// Prints only when a terminal is attached. Startup banners and reload
/// confirmations are nice-to-have context, not something worth interrupting
/// a silent launch with a dialog.
pub fn console_line(message: &str) {
    if has_console() {
        println!("{message}");
    }
}
