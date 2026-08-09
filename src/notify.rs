//! Telling the user something outside the tray: the parent terminal's console
//! and modal dialogs.
//!
//! WinRemap is a `windows` subsystem binary (ADR 0029), so it never opens a
//! console window of its own. Output only reaches a terminal when the user
//! started it from one, and anything the user *must* see has to become a
//! dialog when there is no terminal. Both halves of that live here so the
//! `unsafe` stays out of main.rs, by the same reasoning as ADR 0009
//! (AGENTS.md invariant 3, ADR 0031).
//!
//! A borrowed console is also a leash — closing the terminal kills everything
//! attached to it — so the attach is not permanent: [`detach_console`] hands
//! it back once startup output is done (ADR 0062).

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_DISK,
    FILE_TYPE_PIPE, GetFileType, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole, CONSOLE_MODE, ENABLE_EXTENDED_FLAGS,
    ENABLE_QUICK_EDIT_MODE, FreeConsole, GetConsoleMode, GetStdHandle, STD_ERROR_HANDLE,
    STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleMode, SetConsoleTitleW,
    SetStdHandle,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MESSAGEBOX_STYLE,
    MessageBoxW, SM_SHUTTINGDOWN,
};
use windows::core::{HSTRING, PCWSTR, w};

/// Whether stdout/stderr reach a terminal. Read from the message loop and the
/// tray callback, so it is an atomic rather than a `OnceLock<bool>`.
static HAS_CONSOLE: AtomicBool = AtomicBool::new(false);

/// Whether we borrowed the launcher's console, so [`detach_console`] knows
/// there is one to give back.
static ATTACHED: AtomicBool = AtomicBool::new(false);

/// Whether the launcher set *both* standard handles itself — a file or a pipe
/// that survives letting the console go. Recorded before the attach adopts
/// anything, since after that the handles no longer say who set them.
static LAUNCHER_OWNS_OUTPUT: AtomicBool = AtomicBool::new(false);

/// Whether the console is one WinRemap opened for itself (`--debug`), rather
/// than the launcher's. Only such a console can be waited on at exit: the
/// launcher's outlives us anyway (ADR 0068).
static OWNS_CONSOLE: AtomicBool = AtomicBool::new(false);

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
    LAUNCHER_OWNS_OUTPUT.store(
        redirected && std_handle_is_set(STD_ERROR_HANDLE),
        Ordering::Relaxed,
    );
    // SAFETY: no arguments to get wrong; failure just means the launcher had
    // no console (Explorer, autostart), which is the expected silent case.
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_ok();
    ATTACHED.store(attached, Ordering::Relaxed);
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

/// Gives the launcher's console back, so closing that terminal no longer
/// closes WinRemap.
///
/// Call once, when startup is over and the process is about to go resident,
/// and only when the console is not a log destination — `--debug` streams to
/// it for the whole run, so that mode stays attached and keeps dying with the
/// terminal (ADR 0062).
///
/// The console is what ties the two together, not the parent process: closing
/// a console window sends `CTRL_CLOSE_EVENT` to every process attached to it
/// and terminates them once the handler returns, which no handler can refuse.
/// Letting go is the only way out.
pub fn detach_console() {
    if !ATTACHED.swap(false, Ordering::Relaxed) {
        return;
    }
    // Silenced first: the adopted CONOUT$ handles are about to dangle, and
    // `println!` panics when a write fails.
    HAS_CONSOLE.store(false, Ordering::Relaxed);
    // SAFETY: no arguments to get wrong; the only failure is not being
    // attached, which the swap above already ruled out.
    let _ = unsafe { FreeConsole() };
    // Output stays on only when every stream is the launcher's own file or
    // pipe, which FreeConsole does not touch. Otherwise messages take the
    // dialog path from here on, exactly as for an Explorer launch.
    HAS_CONSOLE.store(
        LAUNCHER_OWNS_OUTPUT.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
}

/// Opens a console of WinRemap's own for the `--debug` transcript, and reports
/// whether there now is one.
///
/// The alternative — writing into the console of the shell that launched us —
/// shares one screen buffer with that shell. WinRemap is a `windows` subsystem
/// binary, so the shell does not wait for it: the prompt comes straight back
/// and repaints itself on top of the log, and the two overwrite each other.
/// A console of our own has no second writer, and it exists from the first
/// line of startup. The tray's log window cannot cover this — it can only be
/// opened once startup is over, and it vanishes with the process at the end
/// (ADR 0068, which supersedes that part of ADR 0029).
///
/// Does nothing when stdout is a file or a pipe: a caller who redirected the
/// transcript asked for the bytes, not a window. The UI tests read `--debug`
/// exactly that way, which is why the check comes first.
pub fn open_debug_console() -> bool {
    if stdout_is_captured() {
        return false;
    }
    // A process has at most one console, so the launcher's has to go before
    // ours can arrive.
    if ATTACHED.swap(false, Ordering::Relaxed) {
        // SAFETY: no arguments to get wrong; the swap above established that
        // there is a console to release.
        let _ = unsafe { FreeConsole() };
    }
    // SAFETY: no arguments to get wrong. On failure we are left with no
    // console at all, which the return value reports.
    if unsafe { AllocConsole() }.is_err() {
        HAS_CONSOLE.store(
            LAUNCHER_OWNS_OUTPUT.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        return false;
    }
    // Unconditional, unlike the attach path: whatever these pointed at before
    // belonged to the console we just left.
    point_std_handle_at(STD_OUTPUT_HANDLE, w!("CONOUT$"));
    point_std_handle_at(STD_ERROR_HANDLE, w!("CONOUT$"));
    // Input too — the wait at exit reads a line from it.
    point_std_handle_at(STD_INPUT_HANDLE, w!("CONIN$"));
    disable_quick_edit();
    // SAFETY: a static wide literal, valid for the duration of the call.
    let _ = unsafe { SetConsoleTitleW(w!("WinRemap --debug")) };
    OWNS_CONSOLE.store(true, Ordering::Relaxed);
    HAS_CONSOLE.store(true, Ordering::Relaxed);
    true
}

/// Turns QuickEdit off on the console we just opened.
///
/// A fresh console has QuickEdit on, which turns a single click in the window
/// into a text selection — and **a console in selection mode blocks every
/// write to it**. The thread that blocks is the one draining the log, so it
/// stops pumping messages; the low-level hook then misses
/// `LowLevelHooksTimeout` (300 ms by default) and Windows starts dropping its
/// calls. **One stray click in the `--debug` window stops the remapping**,
/// which is invariant 1 (never stall the hook) losing to a convenience.
///
/// Selecting text is still possible from the window menu (Alt+Space → 編集 →
/// 範囲指定); the click shortcut is what goes. Found in the v0.8 acceptance,
/// 2026-08-03 — it cost two retries of M-1 before the cause was clear
/// (ADR 0071).
fn disable_quick_edit() {
    // SAFETY: STD_INPUT_HANDLE is a documented constant, and the handle is
    // only read here.
    let Ok(input) = (unsafe { GetStdHandle(STD_INPUT_HANDLE) }) else {
        return;
    };
    let mut mode = CONSOLE_MODE::default();
    // SAFETY: `input` is the console input handle installed just above, and
    // `mode` is a live local for the duration of the call.
    if unsafe { GetConsoleMode(input, &mut mode) }.is_err() {
        return;
    }
    // ENABLE_EXTENDED_FLAGS has to travel with the change: without it
    // SetConsoleMode ignores the QuickEdit bit entirely.
    let mode = (mode & !ENABLE_QUICK_EDIT_MODE) | ENABLE_EXTENDED_FLAGS;
    // SAFETY: same handle; every other bit is carried over from the mode we
    // just read, so the line input the exit wait needs stays on.
    let _ = unsafe { SetConsoleMode(input, mode) };
}

/// Writes a line to stdout, and **keeps going when the write fails**.
///
/// `println!` panics on a failed write, and the write can fail for reasons
/// that are none of WinRemap's doing. PowerShell's `>` hands a native process
/// a pipe and closes it the moment the command "finishes" — which, for a
/// process that goes resident, is long before the last log line. The panic
/// took the whole tray app down with it (v0.8 acceptance C-4, 2026-08-03).
///
/// A transcript that cannot be delivered is a lost transcript; it is not a
/// reason to stop remapping keys (ADR 0071).
fn print_line(message: &str) {
    let _ = writeln!(std::io::stdout(), "{message}");
}

/// [`print_line`] for the error stream.
fn print_error_line(message: &str) {
    let _ = writeln!(std::io::stderr(), "{message}");
}

/// Holds the `--debug` console open after WinRemap is done, so its last lines
/// can be read.
///
/// A console from `AllocConsole` belongs to this process alone and closes the
/// instant the process does — which would leave the shutdown transcript on
/// screen for zero milliseconds. **This wait is what makes "what happens at
/// exit" observable at all**, and without it the console of ADR 0068 would
/// only have solved the startup half.
///
/// Returns immediately when the console is not ours: the launcher's outlives
/// us anyway, and a redirected run has nobody to press a key.
pub fn wait_for_debug_console() {
    if !OWNS_CONSOLE.load(Ordering::Relaxed) || session_is_ending() {
        return;
    }
    print_line(crate::i18n::t().debug_console_wait);
    // This returns when Enter is pressed. Closing the window instead sends
    // CTRL_CLOSE_EVENT, which ends the process — no handler can refuse that
    // (ADR 0062), and for a debug session it is the intended way out.
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}

/// Whether Windows is ending the session (sign-out, shutdown, restart).
///
/// Checked before the wait above: there is nobody at the keyboard to close a
/// window, and a process that will not finish is one that Windows has to sit
/// out its shutdown timeout on.
fn session_is_ending() -> bool {
    // SAFETY: SM_SHUTTINGDOWN is a documented index; the call has no
    // preconditions.
    unsafe { GetSystemMetrics(SM_SHUTTINGDOWN) != 0 }
}

/// Whether stdout goes somewhere a console cannot stand in for — a file or a
/// pipe.
///
/// The handle alone does not say: an inherited console and a `> out.txt` both
/// look "set", so the question is what kind of object it is.
fn stdout_is_captured() -> bool {
    // SAFETY: `STD_OUTPUT_HANDLE` is one of the documented STD_* constants.
    let Ok(handle) = (unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }) else {
        return false;
    };
    if handle.is_invalid() {
        return false;
    }
    // SAFETY: any handle value is acceptable; unknown ones report FILE_TYPE_UNKNOWN.
    let kind = unsafe { GetFileType(handle) };
    kind == FILE_TYPE_DISK || kind == FILE_TYPE_PIPE
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
    point_std_handle_at(which, w!("CONOUT$"));
}

/// Points a standard handle at a console device (`CONOUT$` / `CONIN$`),
/// whatever it held before. Unlike [`adopt_console_handle`] this overwrites,
/// which is what opening our own console needs: the old value refers to a
/// console this process has already left.
fn point_std_handle_at(which: STD_HANDLE, device: PCWSTR) {
    // SAFETY: the device names are the console's own; the handle is handed to
    // SetStdHandle, which takes ownership for the life of the process.
    let console = unsafe {
        CreateFileW(
            device,
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
    crate::gui::log::push(message);
    if has_console() {
        print_error_line(message);
    } else {
        message_box(message, MB_ICONERROR);
    }
}

/// Same routing as [`error`] for output that is informational rather than a
/// failure — `--help` and `--version` when launched without a terminal.
pub fn info(message: &str) {
    if has_console() {
        print_line(message);
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

/// Prints only when a terminal is attached.
///
/// The log module is the only caller: it decides *whether* a line is wanted
/// on the console (`--debug`) and what it looks like, and this decides
/// whether there is anywhere to put it (ADR 0058). Anything else that wants
/// the user's attention goes through [`error`] or [`info`], which have a
/// dialog to fall back on.
pub fn console_line(message: &str) {
    if has_console() {
        print_line(message);
    }
}
