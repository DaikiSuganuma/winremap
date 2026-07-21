//! Win32 calls the GUI needs and egui does not expose (ADR 0038).
//!
//! The GUI is otherwise `unsafe`-free; everything that needs a raw handle or a
//! shell API lives here, which is why this file is on the unsafe allowlist
//! (AGENTS.md invariant 3). Nothing here runs on the hook's path, and a
//! failure only costs an icon or an unopened editor — never remapping.

use std::path::Path;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GetSystemMetrics, HICON, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR,
    LR_SHARED, LoadImageW, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON, SW_SHOWNORMAL,
    SendMessageW, WM_SETICON,
};
use windows::core::{BOOL, HSTRING, PCWSTR, w};

/// The exe's own icon resource, embedded by build.rs as ordinal 1 (ADR 0010).
/// It is a multi-size .ico, which is the whole point: Windows picks the face
/// drawn for the size it is about to render.
const ICON_ORDINAL: u16 = 1;

/// Gives every window this thread owns the app icon.
///
/// egui hands winit a single bitmap and winit installs it as `ICON_SMALL`
/// only, leaving `ICON_BIG` unset — so Windows stretches one small bitmap
/// wherever it wants a large icon, which is what made the title-bar and Task
/// Manager icons look broken (ADR 0038). Loading per-size faces from the
/// embedded .ico and setting both slots is the fix.
///
/// Enumerating the thread's windows rather than tracking handles keeps this
/// independent of how many viewports exist: every eframe window lives on the
/// GUI thread, and re-setting an icon a window already has is a no-op.
pub fn set_window_icons() {
    // SAFETY: the callback only calls documented Win32 on the handle it is
    // given, and takes no state from us.
    unsafe {
        let _ = EnumThreadWindows(GetCurrentThreadId(), Some(apply_icons), LPARAM(0));
    }
}

unsafe extern "system" fn apply_icons(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    // SAFETY: SM_* are documented metric ids; the call cannot fail meaningfully
    // (a zero result just yields LoadImageW's default size).
    let (small, big) = unsafe {
        (
            (GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON)),
            (GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON)),
        )
    };
    if let Some(icon) = load_icon(small) {
        set_icon(hwnd, ICON_SMALL, icon);
    }
    if let Some(icon) = load_icon(big) {
        set_icon(hwnd, ICON_BIG, icon);
    }
    true.into()
}

/// Loads the embedded icon at an exact pixel size. `LR_SHARED` means the
/// system owns the handle and it must never be destroyed — which is what we
/// want, since the windows outlive this call.
fn load_icon((width, height): (i32, i32)) -> Option<HICON> {
    // SAFETY: the module handle is our own exe; the resource ordinal is the
    // one build.rs embeds. A missing resource returns Err, handled here.
    let handle = unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        LoadImageW(
            Some(instance.into()),
            PCWSTR(ICON_ORDINAL as usize as *const u16),
            IMAGE_ICON,
            width,
            height,
            LR_DEFAULTCOLOR | LR_SHARED,
        )
        .ok()?
    };
    (!handle.is_invalid()).then_some(HICON(handle.0))
}

fn set_icon(hwnd: HWND, which: u32, icon: HICON) {
    // SAFETY: hwnd came from the enumeration and is live for this callback;
    // WM_SETICON takes ownership of nothing (LR_SHARED handles are the
    // system's). The message is sent to our own thread, so it cannot block.
    unsafe {
        SendMessageW(
            hwnd,
            WM_SETICON,
            Some(WPARAM(which as usize)),
            Some(LPARAM(icon.0 as isize)),
        );
    }
}

/// Hands a file to whatever the user associated with its extension.
///
/// Replaces `cmd /C start`, which needs standard handles this process does not
/// have since it became a windows-subsystem binary (ADR 0029) — that is why
/// the settings window's "open in text editor" did nothing (ADR 0038).
/// Returns whether the shell accepted the request.
pub fn open_in_default_editor(path: &Path) -> bool {
    shell_open(&HSTRING::from(path.as_os_str()))
}

/// Opens a URL in the default browser.
///
/// eframe only follows `ui.hyperlink_to` when built with its `webbrowser`
/// feature, which this build does not enable, so the link would silently do
/// nothing. The shell already knows how to do this.
pub fn open_url(url: &str) -> bool {
    shell_open(&HSTRING::from(url))
}

/// The shell's "open" verb on a file path or a URL.
fn shell_open(target: &HSTRING) -> bool {
    // SAFETY: both strings outlive the call; a null owner window is valid and
    // gives a top-level shell action, which is what a tray app wants.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a fake HINSTANCE: > 32 means success.
    result.0 as usize > 32
}
