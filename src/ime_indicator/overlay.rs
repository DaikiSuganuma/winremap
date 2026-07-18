//! Overlay window and Win32 plumbing for the indicator thread.
//!
//! Owns everything `unsafe` on that thread: the layered click-through panel
//! (design doc §3.3), its timers, and the thread-message helpers the safe
//! orchestration in mod.rs builds on. The panel is a rounded square drawing
//! a large "あ", shown without ever taking focus or input:
//! WS_EX_TRANSPARENT (clicks fall through), WS_EX_NOACTIVATE (no focus
//! steal), WS_EX_TOPMOST, WS_EX_TOOLWINDOW (no taskbar/Alt-Tab entry), and
//! WS_EX_LAYERED with uniform alpha for translucency and the fade-out.

use std::cell::Cell;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontIndirectW, CreateRoundRectRgn, CreateSolidBrush, DT_CENTER,
    DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, EndPaint, FillRect, InvalidateRect,
    LOGFONTW, PAINTSTRUCT, SelectObject, SetBkMode, SetTextColor, SetWindowRgn, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, GetWindowRect,
    HWND_TOPMOST, KillTimer, LWA_ALPHA, MSG, PostThreadMessageW, RegisterClassW, SW_HIDE,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow,
    TranslateMessage, WM_PAINT, WM_QUIT, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use winremap::ime_indicator_settings::IndicatorSettings;

const CLASS_NAME: PCWSTR = w!("winremap.ime_indicator");

/// Overlay-window timers: visible hold, then the fade-out steps.
const TIMER_HOLD: usize = 1;
const TIMER_FADE: usize = 2;
const FADE_INTERVAL_MS: u32 = 25;

/// Panel colors (fixed v1 design): near-black background, white glyph.
const BG_COLOR: COLORREF = COLORREF(0x00201C1C);
const TEXT_COLOR: COLORREF = COLORREF(0x00FFFFFF);

#[derive(Clone, Copy)]
struct Panel {
    size: i32,
    opacity: u8,
}

thread_local! {
    /// What the window procedure paints/fades with. Thread-local is enough:
    /// the overlay window and its wndproc live on the indicator thread only.
    static PANEL: Cell<Panel> = const {
        Cell::new(Panel {
            size: 96,
            opacity: 200,
        })
    };
    /// Current alpha while TIMER_FADE is stepping it down.
    static FADE_ALPHA: Cell<u8> = const { Cell::new(0) };
}

/// The (hidden) overlay window; created and used on the indicator thread.
pub struct Overlay {
    hwnd: HWND,
}

impl Overlay {
    pub fn create() -> windows::core::Result<Self> {
        // SAFETY: wndproc is a static fn valid for the process lifetime.
        // RegisterClassW failing because the class already exists (a
        // restarted indicator thread) is fine — CreateWindowExW below is
        // what actually has to succeed.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            let _ = RegisterClassW(&class);
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW,
                CLASS_NAME,
                w!("winremap IME indicator"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        Ok(Self { hwnd })
    }

    /// Positions the panel at the center of `target` and (re)starts the
    /// show → hold → fade cycle. Does nothing when the target's rectangle
    /// cannot be determined (e.g. it just closed).
    pub fn show(&self, target: isize, settings: &IndicatorSettings) {
        let Some((center_x, center_y)) = target_center(target) else {
            return;
        };
        let size = settings.size as i32;
        PANEL.set(Panel {
            size,
            opacity: settings.opacity,
        });
        let x = center_x - size / 2;
        let y = center_y - size / 2;
        // SAFETY: self.hwnd is our live window. The region handle created
        // here is owned by the system after SetWindowRgn.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_HOLD);
            let _ = KillTimer(Some(self.hwnd), TIMER_FADE);
            let radius = size / 4;
            let region = CreateRoundRectRgn(0, 0, size + 1, size + 1, radius, radius);
            let _ = SetWindowRgn(self.hwnd, Some(region), false);
            let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), settings.opacity, LWA_ALPHA);
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                size,
                size,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = InvalidateRect(Some(self.hwnd), None, true);
            SetTimer(Some(self.hwnd), TIMER_HOLD, settings.duration_ms, None);
        }
    }

    pub fn hide(&self) {
        // SAFETY: self.hwnd is our live window; hiding twice is harmless.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_HOLD);
            let _ = KillTimer(Some(self.hwnd), TIMER_FADE);
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        // SAFETY: created on this thread (DestroyWindow requirement) and
        // destroyed exactly once here.
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

/// Visual center of `target`. Prefers the DWM frame bounds: GetWindowRect
/// includes the invisible resize borders, which would skew the center
/// (design doc §3.3); it stays as the fallback for windows DWM rejects.
fn target_center(target: isize) -> Option<(i32, i32)> {
    if target == 0 {
        return None;
    }
    let hwnd = HWND(target as *mut core::ffi::c_void);
    let mut rect = RECT::default();
    // SAFETY: rect is a live local of exactly the size passed; a stale hwnd
    // makes the calls fail, which the `ok` check handles.
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut rect).cast(),
            size_of::<RECT>() as u32,
        )
        .is_ok()
            || GetWindowRect(hwnd, &mut rect).is_ok()
    };
    if !ok {
        return None;
    }
    Some(((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2))
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            on_timer(hwnd, wparam.0);
            LRESULT(0)
        }
        // SAFETY: forwarding unchanged arguments as the contract requires.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Fills the panel and draws a centered "あ" scaled to the panel size.
fn paint(hwnd: HWND) {
    let panel = PANEL.get();
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: hwnd is our live window; every GDI object created here is
    // deselected/deleted before EndPaint closes the paint session.
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        if hdc.is_invalid() {
            return;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: panel.size,
            bottom: panel.size,
        };
        let brush = CreateSolidBrush(BG_COLOR);
        let _ = FillRect(hdc, &rect, brush);
        let _ = DeleteObject(brush.into());

        let mut font_spec = LOGFONTW {
            // Negative = character height, so the glyph scales with the
            // panel regardless of DPI virtualization.
            lfHeight: -(panel.size * 55 / 100),
            lfWeight: 600, // semibold reads better at high translucency
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
        let mut glyph = [0x3042u16]; // "あ"
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

/// Hold expiry starts the fade; each fade tick lowers the uniform alpha
/// until the window hides.
fn on_timer(hwnd: HWND, timer_id: usize) {
    match timer_id {
        TIMER_HOLD => {
            FADE_ALPHA.set(PANEL.get().opacity);
            // SAFETY: hwnd is our live window; swapping the hold timer for
            // the fade timer on the same window.
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_HOLD);
                SetTimer(Some(hwnd), TIMER_FADE, FADE_INTERVAL_MS, None);
            }
        }
        TIMER_FADE => {
            let step = (PANEL.get().opacity / 12).max(8);
            let alpha = FADE_ALPHA.get().saturating_sub(step);
            FADE_ALPHA.set(alpha);
            // SAFETY: hwnd is our live window.
            unsafe {
                if alpha == 0 {
                    let _ = KillTimer(Some(hwnd), TIMER_FADE);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                } else {
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
                }
            }
        }
        _ => {}
    }
}

// ---- thread-message plumbing used by mod.rs -------------------------------

pub fn current_thread_id() -> u32 {
    // SAFETY: no preconditions.
    unsafe { GetCurrentThreadId() }
}

/// Non-blocking post; failure (queue full, thread gone) is deliberately
/// ignored — a lost poke only means a missed indicator flash.
pub fn post_to_thread(tid: u32, message: u32) {
    // SAFETY: posting to a thread queue is safe for any tid; stale ids fail.
    let _ = unsafe { PostThreadMessageW(tid, message, WPARAM(0), LPARAM(0)) };
}

pub fn post_quit_to(tid: u32) {
    post_to_thread(tid, WM_QUIT);
}

/// One-shot thread timer (no window); returns the system-chosen id.
pub fn set_thread_timer(delay_ms: u32) -> usize {
    // SAFETY: no window handle means a plain thread timer delivered to the
    // message loop below.
    unsafe { SetTimer(None, 0, delay_ms, None) }
}

pub fn kill_thread_timer(timer_id: usize) {
    // SAFETY: killing an already-fired timer id is harmless (returns Err).
    let _ = unsafe { KillTimer(None, timer_id) };
}

/// Pumps this thread's queue: window messages (paint/fade timers) are
/// dispatched to the wndproc, thread messages are returned to the caller as
/// `(message, wParam)`. `None` means WM_QUIT — time to shut down.
pub fn next_thread_message() -> Option<(u32, usize)> {
    let mut msg = MSG::default();
    // SAFETY: msg is a live local; `.0 > 0` also stops on the -1 error
    // return, which as_bool() would misread as "keep going".
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        if msg.hwnd.is_invalid() {
            return Some((msg.message, msg.wParam.0));
        }
        // SAFETY: msg was filled in by the successful GetMessageW above.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    None
}
