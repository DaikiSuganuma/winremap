//! Phase I1 probe: polls the foreground window's IME open status once per
//! second and prints one line per sample (docs/09_ime-indicator-plan.md).
//!
//! Run with `cargo run --example ime_probe`, switch between apps, and toggle
//! the IME; a leading `*` marks a status change since the previous sample.
//! The goal is to verify that `IMC_GETOPENSTATUS` reflects the real IME state
//! on this machine's IME (design doc §6-1) before the feature is implemented.
//!
//! Standalone on purpose — it duplicates the exe-name lookup from
//! src/window.rs instead of touching the remapper, so it stays usable as a
//! regression check no matter how the main code evolves.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, SMTO_ABORTIFHUNG, SendMessageTimeoutW,
    WM_IME_CONTROL,
};
use windows::core::PWSTR;

/// Cross-process queries must be bounded so a hung target cannot stall us
/// (design doc §3.2 mandates the timeout variant of SendMessage).
const QUERY_TIMEOUT_MS: u32 = 100;

/// Not exported by the `windows` crate (it only has IMC_SETOPENSTATUS);
/// value per https://learn.microsoft.com/en-us/windows/win32/intl/wm-ime-control
const IMC_GETOPENSTATUS: usize = 0x0005;

fn main() {
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
