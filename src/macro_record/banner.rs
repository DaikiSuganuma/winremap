//! The recording/replay banner window and the Win32 plumbing for the
//! macro-record thread.
//!
//! Owns everything `unsafe` on that thread (AGENTS.md invariant 3,
//! ADR 0044). The banner is a layered click-through strip at the bottom of
//! the primary monitor, shown without ever taking focus or input:
//! WS_EX_TRANSPARENT (clicks fall through), WS_EX_NOACTIVATE (no focus
//! steal), WS_EX_TOPMOST, WS_EX_TOOLWINDOW (no taskbar/Alt-Tab entry), and
//! WS_EX_LAYERED for translucency.
//!
//! Deliberately not the IME indicator's overlay (design doc §6.1): that
//! panel is built to hold and then fade out, while this one must stay up
//! for as long as a recording runs (ADR 0043 decision 3). It also carries a
//! variable-length line rather than a single glyph, so the sizing and
//! painting differ throughout. Only the visual style is shared.

use std::cell::RefCell;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontIndirectW, CreateRoundRectRgn, CreateSolidBrush, DT_CENTER,
    DT_END_ELLIPSIS, DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC,
    GetTextExtentPoint32W, HFONT, InvalidateRect, LOGFONTW, PAINTSTRUCT, ReleaseDC, SelectObject,
    SetBkMode, SetTextColor, SetWindowRgn, TRANSPARENT, UpdateWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    HWND_TOPMOST, KillTimer, LWA_ALPHA, MSG, PostThreadMessageW, RegisterClassW, SPI_GETWORKAREA,
    SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow, SystemParametersInfoW,
    TranslateMessage, WM_PAINT, WM_QUIT, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{PCWSTR, w};

const CLASS_NAME: PCWSTR = w!("winremap.macro_record_banner");

/// Hides the banner after a message that is not a state — the limit notice
/// and the cancellation notice (design doc §6.3).
const TIMER_HIDE: usize = 1;

/// Panel colors, matching the IME indicator's palette so the two read as
/// parts of the same application.
const BG_COLOR: COLORREF = COLORREF(0x00201C1C);
const TEXT_COLOR: COLORREF = COLORREF(0x00FFFFFF);
/// Fixed: unlike the indicator's panel this is a status line, not a
/// decoration, so there is nothing to tune per config.
const OPACITY: u8 = 230;

/// LOGFONT height (negative = character height) and the space around it.
const FONT_HEIGHT: i32 = -18;
const PADDING_X: i32 = 20;
const HEIGHT: i32 = 44;
/// Gap between the banner and the bottom of the work area, so it clears the
/// taskbar without sitting flush against it.
const MARGIN_BOTTOM: i32 = 24;
/// Never take more than this share of the work area's width; longer lines
/// get an ellipsis instead.
const MAX_WIDTH_PERCENT: i32 = 80;

thread_local! {
    /// The line being displayed. Thread-local is enough: the window and its
    /// window procedure live on the macro-record thread only.
    static TEXT: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
}

pub struct Banner {
    hwnd: HWND,
}

impl Banner {
    pub fn create() -> windows::core::Result<Self> {
        // SAFETY: wndproc is a static fn valid for the process lifetime.
        // RegisterClassW failing because the class already exists (a
        // restarted macro-record thread) is fine — CreateWindowExW below is
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
                w!("winremap macro recording"),
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

    /// Shows `text` at the bottom of the primary monitor. `auto_hide_ms`
    /// hides it again after a delay; `None` leaves it up until told
    /// otherwise, which is what a running recording needs.
    ///
    /// Paints synchronously before returning, because the caller may go
    /// straight into a paced replay and stop pumping messages for as long
    /// as it lasts (design doc §6.4).
    pub fn show(&self, text: &str, auto_hide_ms: Option<u32>) {
        let text: Vec<u16> = text.encode_utf16().collect();
        let work = work_area();
        let max_width = (work.right - work.left) * MAX_WIDTH_PERCENT / 100;
        let width = (self.measure(&text) + PADDING_X * 2).clamp(0, max_width.max(1));
        TEXT.with(|slot| *slot.borrow_mut() = text);
        let x = (work.left + work.right) / 2 - width / 2;
        let y = work.bottom - HEIGHT - MARGIN_BOTTOM;
        // SAFETY: self.hwnd is our live window. The region handle created
        // here is owned by the system after SetWindowRgn.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_HIDE);
            let radius = HEIGHT / 2;
            let region = CreateRoundRectRgn(0, 0, width + 1, HEIGHT + 1, radius, radius);
            let _ = SetWindowRgn(self.hwnd, Some(region), false);
            let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), OPACITY, LWA_ALPHA);
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                width,
                HEIGHT,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = InvalidateRect(Some(self.hwnd), None, true);
            let _ = UpdateWindow(self.hwnd);
            if let Some(delay) = auto_hide_ms {
                SetTimer(Some(self.hwnd), TIMER_HIDE, delay, None);
            }
        }
    }

    pub fn hide(&self) {
        // SAFETY: self.hwnd is our live window; hiding twice is harmless.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_HIDE);
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// Pixel width of `text` at the banner font size.
    fn measure(&self, text: &[u16]) -> i32 {
        // SAFETY: the DC is acquired/released in pairs and the font is
        // deselected and deleted before returning; extent is a live local.
        unsafe {
            let hdc = GetDC(Some(self.hwnd));
            if hdc.is_invalid() {
                return 0;
            }
            let font = create_banner_font();
            let previous = SelectObject(hdc, font.into());
            let mut extent = SIZE::default();
            let _ = GetTextExtentPoint32W(hdc, text, &mut extent);
            SelectObject(hdc, previous);
            let _ = DeleteObject(font.into());
            ReleaseDC(Some(self.hwnd), hdc);
            extent.cx
        }
    }
}

impl Drop for Banner {
    fn drop(&mut self) {
        // SAFETY: created on this thread (DestroyWindow requirement) and
        // destroyed exactly once here.
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

/// The primary monitor's work area — the screen minus the taskbar. Falls
/// back to a plausible rectangle if the query fails, which only misplaces
/// the banner rather than losing it.
fn work_area() -> RECT {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };
    // SAFETY: rect is a live local of the size SPI_GETWORKAREA writes; a
    // failure leaves the fallback above untouched.
    let _ = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&raw mut rect).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    rect
}

/// Yu Gothic UI at the banner size; falls back via GDI font substitution if
/// the face is missing.
///
/// SAFETY contract: caller deletes the returned font handle.
unsafe fn create_banner_font() -> HFONT {
    let mut font_spec = LOGFONTW {
        lfHeight: FONT_HEIGHT,
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
    // SAFETY: font_spec is a live, fully initialized local.
    unsafe { CreateFontIndirectW(&font_spec) }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_HIDE => {
            // SAFETY: hwnd is our live window.
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_HIDE);
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            LRESULT(0)
        }
        // SAFETY: forwarding unchanged arguments as the contract requires.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn paint(hwnd: HWND) {
    TEXT.with(|text| {
        let text = text.borrow();
        let mut ps = PAINTSTRUCT::default();
        // SAFETY: hwnd is our live window; every GDI object created here is
        // deselected/deleted before EndPaint closes the paint session.
        unsafe {
            let hdc = BeginPaint(hwnd, &mut ps);
            if hdc.is_invalid() {
                return;
            }
            // The client rect, not ps.rcPaint: an uncovered window repaints
            // only the invalid part, and sizing the text to that would
            // shift the line every time something passed over the banner.
            let mut full = RECT::default();
            let _ = GetClientRect(hwnd, &mut full);
            let brush = CreateSolidBrush(BG_COLOR);
            let _ = FillRect(hdc, &full, brush);
            let _ = DeleteObject(brush.into());
            let _ = SetBkMode(hdc, TRANSPARENT);

            let font = create_banner_font();
            let previous = SelectObject(hdc, font.into());
            let _ = SetTextColor(hdc, TEXT_COLOR);
            // DrawTextW may modify the buffer for the ellipsis case, so draw
            // from a scratch copy.
            let mut line = text.clone();
            let mut rect = RECT {
                left: PADDING_X,
                top: 0,
                right: full.right - PADDING_X,
                bottom: full.bottom,
            };
            let _ = DrawTextW(
                hdc,
                &mut line,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            let _ = SelectObject(hdc, previous);
            let _ = DeleteObject(font.into());
            let _ = EndPaint(hwnd, &ps);
        }
    });
}

// ---- thread-message plumbing used by mod.rs -------------------------------

pub fn current_thread_id() -> u32 {
    // SAFETY: no preconditions.
    unsafe { GetCurrentThreadId() }
}

/// Non-blocking post; failure (queue full, thread gone) is deliberately
/// ignored. Called from the hook callback, so it must never wait
/// (invariant 2, explicit exception 4 / ADR 0044).
pub fn post_to_thread(tid: u32, message: u32, wparam: usize) {
    // SAFETY: posting to a thread queue is safe for any tid; stale ids fail.
    let _ = unsafe { PostThreadMessageW(tid, message, WPARAM(wparam), LPARAM(0)) };
}

pub fn post_quit_to(tid: u32) {
    post_to_thread(tid, WM_QUIT, 0);
}

/// Pumps this thread's queue: window messages (paint, hide timer) are
/// dispatched to the window procedure, thread messages are returned to the
/// caller as `(message, wParam)`. `None` means WM_QUIT — time to shut down.
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
