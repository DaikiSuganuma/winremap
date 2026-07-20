//! Phase I1 probe: polls the foreground window's IME open status once per
//! second and prints one line per sample (docs/v0.1/06_ime-indicator-plan.md).
//!
//! Run with `cargo run --example ime_probe`, switch between apps, and toggle
//! the IME; a leading `*` marks a status change since the previous sample.
//! The goal is to verify that `IMC_GETOPENSTATUS` reflects the real IME state
//! on this machine's IME (design doc §6-1) before the feature is implemented.
//!
//! `--overlay` runs a visual self-test instead: it shows the same kind of
//! layered panel as src/ime_indicator/overlay.rs at the center of the
//! current foreground window for 3 seconds, independent of any IME state —
//! isolating "is the overlay visible at all?" from "does IME detection
//! work?" (plan Phase I3 verification).
//!
//! Note that this probe queries the foreground window's own thread only,
//! which is what src/ime_indicator/detect.rs did until ADR 0033. Apps whose
//! editor runs on a second UI thread (WinUI 3, e.g. the Windows 11 Notepad)
//! therefore read OFF here while the real status is ON — that difference is
//! itself useful when diagnosing, but do not read a plain OFF from this tool
//! as "the indicator should show nothing".
//!
//! Standalone on purpose — it duplicates the exe-name lookup from
//! src/window.rs (and the overlay drawing) instead of touching the remapper,
//! so it stays usable as a regression check no matter how the code evolves.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    COLORREF, CloseHandle, HWND, LPARAM, LRESULT, MAX_PATH, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontIndirectW, CreateRoundRectRgn, CreateSolidBrush, DT_CENTER,
    DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, EndPaint, FillRect, LOGFONTW, PAINTSTRUCT,
    SelectObject, SetBkMode, SetTextColor, SetWindowRgn, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetWindowRect,
    GetWindowThreadProcessId, HWND_TOPMOST, IsWindowVisible, LWA_ALPHA, MSG, PM_REMOVE,
    PeekMessageW, RegisterClassW, SMTO_ABORTIFHUNG, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    SendMessageTimeoutW, SetLayeredWindowAttributes, SetWindowPos, TranslateMessage,
    WM_IME_CONTROL, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{PWSTR, w};

/// Cross-process queries must be bounded so a hung target cannot stall us
/// (design doc §3.2 mandates the timeout variant of SendMessage).
const QUERY_TIMEOUT_MS: u32 = 100;

/// Not exported by the `windows` crate (it only has IMC_SETOPENSTATUS);
/// value per https://learn.microsoft.com/en-us/windows/win32/intl/wm-ime-control
const IMC_GETOPENSTATUS: usize = 0x0005;

fn main() {
    if std::env::args().any(|arg| arg == "--overlay") {
        overlay_selftest();
        return;
    }
    println!("ime_probe: polling the foreground window's IME open status every second.");
    println!("Switch apps and toggle the IME; '*' marks a change. Ctrl+C to quit.");
    let mut last_status: Option<Option<bool>> = None;
    loop {
        let sample = sample_foreground();
        let changed = last_status.is_some_and(|prev| prev != sample.status);
        last_status = Some(sample.status);
        let marker = if changed { '*' } else { ' ' };
        let status = match sample.status {
            Some(true) => "ON ",
            Some(false) => "OFF",
            None => "n/a",
        };
        println!(
            "{marker} {:<28} hwnd={:#010x} ime_wnd={:#010x} open={status} ({} us)",
            sample.exe, sample.hwnd, sample.ime_wnd, sample.query_micros
        );
        std::thread::sleep(Duration::from_secs(1));
    }
}

struct Sample {
    exe: String,
    hwnd: usize,
    ime_wnd: usize,
    /// `None` when there is no foreground/IME window or the query failed or
    /// timed out — the cases where the feature would show nothing.
    status: Option<bool>,
    query_micros: u128,
}

fn sample_foreground() -> Sample {
    // SAFETY: no preconditions; a null HWND (e.g. during a UAC prompt) is
    // handled below.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return Sample {
            exe: "<no foreground window>".into(),
            hwnd: 0,
            ime_wnd: 0,
            status: None,
            query_micros: 0,
        };
    }
    let exe = query_exe_name(hwnd).unwrap_or_else(|| "<unknown>".into());
    // SAFETY: hwnd was checked non-null above; a null result (no IME window,
    // e.g. console sessions without IME) is handled below.
    let ime_wnd = unsafe { ImmGetDefaultIMEWnd(hwnd) };
    if ime_wnd.is_invalid() {
        return Sample {
            exe,
            hwnd: hwnd.0 as usize,
            ime_wnd: 0,
            status: None,
            query_micros: 0,
        };
    }
    let started = Instant::now();
    let mut open_status = 0usize;
    // SAFETY: ime_wnd is a window handle owned by another process; even if it
    // dies mid-call the API just fails. open_status outlives the call, and
    // SMTO_ABORTIFHUNG plus the timeout bounds the wait.
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
        exe,
        hwnd: hwnd.0 as usize,
        ime_wnd: ime_wnd.0 as usize,
        status: (sent.0 != 0).then_some(open_status != 0),
        query_micros: started.elapsed().as_micros(),
    }
}

/// Lowercase exe basename for the process owning `hwnd` — same normalization
/// as src/window.rs, so printed names match config `application` values.
fn query_exe_name(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    // SAFETY: hwnd validity was checked by the caller; pid points to a live
    // local.
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
    Some(
        full.rsplit(['\\', '/'])
            .next()
            .unwrap_or(&full)
            .to_ascii_lowercase(),
    )
}

// ---- --overlay visual self-test -------------------------------------------

const PANEL_SIZE: i32 = 96;
const PANEL_OPACITY: u8 = 200;
const BG_COLOR: COLORREF = COLORREF(0x00201C1C);
const TEXT_COLOR: COLORREF = COLORREF(0x00FFFFFF);

/// Mirrors src/ime_indicator/overlay.rs: same extended styles, region, and
/// drawing, but shown unconditionally so the rendering path can be verified
/// without touching the IME.
fn overlay_selftest() {
    println!("overlay self-test: a translucent dark panel with \"\u{3042}\" should appear");
    println!("at the center of the current foreground window for 3 seconds.");
    // SAFETY: straight-line Win32 window setup on this thread; every handle
    // used below is either checked or owned by this function until the end.
    unsafe {
        let foreground = GetForegroundWindow();
        let mut target = RECT::default();
        if foreground.is_invalid() || GetWindowRect(foreground, &mut target).is_err() {
            println!("no foreground window rect; aborting");
            return;
        }
        let instance = GetModuleHandleW(None).expect("own module handle");
        let class = WNDCLASSW {
            lpfnWndProc: Some(overlay_wndproc),
            hInstance: instance.into(),
            lpszClassName: w!("winremap.ime_probe_overlay"),
            ..Default::default()
        };
        let _ = RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("winremap.ime_probe_overlay"),
            w!("ime_probe overlay"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .expect("create overlay window");
        let x = (target.left + target.right) / 2 - PANEL_SIZE / 2;
        let y = (target.top + target.bottom) / 2 - PANEL_SIZE / 2;
        let radius = PANEL_SIZE / 4;
        let region = CreateRoundRectRgn(0, 0, PANEL_SIZE + 1, PANEL_SIZE + 1, radius, radius);
        let region_set = SetWindowRgn(hwnd, Some(region), false);
        let alpha_set = SetLayeredWindowAttributes(hwnd, COLORREF(0), PANEL_OPACITY, LWA_ALPHA);
        let pos_set = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            PANEL_SIZE,
            PANEL_SIZE,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        println!(
            "target=({},{})-({},{})  panel at ({x},{y})  SetWindowRgn={region_set}  \
             SetLayeredWindowAttributes={}  SetWindowPos={}",
            target.left,
            target.top,
            target.right,
            target.bottom,
            alpha_set.is_ok(),
            pos_set.is_ok()
        );

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut reported = false;
        while Instant::now() < deadline {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if !reported {
                // After the first paint pump, report what Windows thinks.
                let mut shown = RECT::default();
                let _ = GetWindowRect(hwnd, &mut shown);
                println!(
                    "IsWindowVisible={}  overlay rect=({},{})-({},{})",
                    IsWindowVisible(hwnd).as_bool(),
                    shown.left,
                    shown.top,
                    shown.right,
                    shown.bottom
                );
                reported = true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    println!("overlay self-test finished.");
}

unsafe extern "system" fn overlay_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_PAINT {
        // SAFETY: hwnd is our live window; GDI objects are deleted before
        // EndPaint closes the session (same drawing as overlay.rs).
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !hdc.is_invalid() {
                let mut rect = RECT {
                    left: 0,
                    top: 0,
                    right: PANEL_SIZE,
                    bottom: PANEL_SIZE,
                };
                let brush = CreateSolidBrush(BG_COLOR);
                let _ = FillRect(hdc, &rect, brush);
                let _ = DeleteObject(brush.into());
                let mut font_spec = LOGFONTW {
                    lfHeight: -(PANEL_SIZE * 55 / 100),
                    lfWeight: 600,
                    ..Default::default()
                };
                for (slot, unit) in font_spec
                    .lfFaceName
                    .iter_mut()
                    .zip("Yu Gothic UI".encode_utf16())
                {
                    *slot = unit;
                }
                let font = CreateFontIndirectW(&font_spec);
                let previous_font = SelectObject(hdc, font.into());
                let _ = SetBkMode(hdc, TRANSPARENT);
                let _ = SetTextColor(hdc, TEXT_COLOR);
                let mut glyph = [0x3042u16];
                let _ = DrawTextW(
                    hdc,
                    &mut glyph,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                let _ = SelectObject(hdc, previous_font);
                let _ = DeleteObject(font.into());
                let _ = EndPaint(hwnd, &ps);
            }
        }
        return LRESULT(0);
    }
    // SAFETY: forwarding unchanged arguments as the contract requires.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
