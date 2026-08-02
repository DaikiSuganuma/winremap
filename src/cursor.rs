//! Tinting the mouse cursor while the IME is on (ADR 0067).
//!
//! `SetSystemCursor` replaces a cursor **for the whole session**, not just
//! for WinRemap's own windows — which is what makes it useful (it shows over
//! every application, including elevated ones, where an overlay window
//! cannot reach) and also what makes it dangerous: the replacement outlives
//! this process. Everything here is arranged around that one fact:
//!
//! - the cursor is only replaced while the IME is **on**, so what is left
//!   behind after a crash is always the tinted one — never a normal-looking
//!   cursor that quietly is not ours (ADR 0067 decision 4);
//! - [`restore`] is cheap and idempotent, so it can be called at startup
//!   without knowing how the last run ended (decision 5);
//! - [`install_crash_restore`] covers the ends that still run code.
//!
//! A sign-out needs nothing: the replacement lives in the session's window
//! station and Windows loads cursors afresh from the registry at logon.
//!
//! The tint keeps the cursor the user actually has — theme, size and shape —
//! and only recolours it: each pixel's brightness becomes the same fraction
//! of the chosen colour, so a black outline stays black and a white body
//! takes the colour.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDIBits, GetObjectW, HBITMAP,
    HGDIOBJ,
};
use windows::Win32::System::Diagnostics::Debug::{EXCEPTION_POINTERS, SetUnhandledExceptionFilter};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CopyIcon, CreateIconIndirect, GetIconInfo, HCURSOR, HICON, ICONINFO, IDC_ARROW, IDC_IBEAM,
    IMAGE_CURSOR, LR_DEFAULTSIZE, LR_SHARED, LoadImageW, OCR_IBEAM, OCR_NORMAL, SPI_SETCURSORS,
    SPIF_SENDCHANGE, SYSTEM_CURSOR_ID, SetSystemCursor, SystemParametersInfoW,
};
use windows::core::{PCWSTR, w};

/// The cursors WinRemap replaces. The arrow alone would leave the state
/// invisible exactly when it matters: while typing, what is under the
/// pointer is usually a text field, and that shows the I-beam.
const REPLACED: [(SYSTEM_CURSOR_ID, PCWSTR); 2] = [(OCR_NORMAL, IDC_ARROW), (OCR_IBEAM, IDC_IBEAM)];

/// Whether the tinted cursors are currently installed. Read from the crash
/// paths, so it is an atomic rather than part of the mutex below.
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Tinted copies, built once per colour. `isize` because a raw `HCURSOR` is
/// not `Send`; the handles are process-wide and used from one thread.
static TINTED: Mutex<Option<Prepared>> = Mutex::new(None);

struct Prepared {
    color: (u8, u8, u8),
    cursors: Vec<(SYSTEM_CURSOR_ID, isize)>,
}

/// Undoes any replacement — ours, or a leftover from a previous run that died
/// before it could undo its own.
///
/// Two steps, and both are needed:
///
/// 1. **Put back the cursors built into `user32.dll`.** `SPI_SETCURSORS`
///    reloads from `HKCU\Control Panel\Cursors`, and on a machine using the
///    stock scheme those values are empty: there is nothing there to reload,
///    so a replacement could simply stay.
/// 2. **Then `SPI_SETCURSORS`.** A user with a cursor scheme of their own has
///    those paths in the registry, and this overwrites step 1 with them — so
///    the stock cursors from step 1 are only what remains when there was no
///    scheme to restore.
///
/// Cheap and idempotent, which is what lets startup call it without knowing
/// how the last run ended (ADR 0067 decision 5). Restoring by handle is not
/// an option: `SetSystemCursor` **destroys** what it is given, so the cursor
/// being replaced cannot be kept for later.
pub fn restore() {
    INSTALLED.store(false, Ordering::Relaxed);
    // SAFETY: a module already loaded into every process; the handle is
    // borrowed, not owned.
    if let Ok(user32) = unsafe { GetModuleHandleW(w!("user32.dll")) } {
        for (id, name) in REPLACED {
            // SAFETY: `name` is the IDC_* atom, which is also this cursor's
            // resource id inside user32; LR_SHARED means the handle is the
            // module's own and must not be destroyed.
            let Ok(stock) =
                (unsafe { LoadImageW(Some(user32.into()), name, IMAGE_CURSOR, 0, 0, LR_SHARED) })
            else {
                continue;
            };
            // SAFETY: a live cursor handle; the copy is what SetSystemCursor
            // takes ownership of, leaving the shared original alone.
            let Ok(copy) = (unsafe { CopyIcon(HICON(stock.0)) }) else {
                continue;
            };
            // SAFETY: `copy` is live and `id` is a documented OCR_* constant.
            let _ = unsafe { SetSystemCursor(HCURSOR(copy.0), id) };
        }
    }
    // SAFETY: SPI_SETCURSORS takes no input buffer; the null pointer and 0
    // are what the documentation prescribes for it.
    let _ = unsafe { SystemParametersInfoW(SPI_SETCURSORS, 0, None, SPIF_SENDCHANGE) };
}

/// Installs or removes the tint. `on` is the IME's open state.
///
/// Building the tinted copies is done once per colour and reused, because
/// this runs on every IME toggle.
pub fn apply(on: bool, color: (u8, u8, u8)) {
    if !on {
        if INSTALLED.swap(false, Ordering::Relaxed) {
            restore();
        }
        return;
    }
    let Ok(mut slot) = TINTED.lock() else { return };
    if slot.as_ref().is_none_or(|p| p.color != color) {
        // Rebuilt from the system's cursors, so it has to happen while ours
        // are not installed — otherwise the tint would be tinted again.
        if INSTALLED.load(Ordering::Relaxed) {
            restore();
        }
        *slot = Some(Prepared {
            color,
            cursors: REPLACED
                .iter()
                .filter_map(|(id, name)| tinted(*name, color).map(|cur| (*id, cur.0 as isize)))
                .collect(),
        });
    }
    let Some(prepared) = slot.as_ref() else {
        return;
    };
    for (id, cursor) in &prepared.cursors {
        // A copy per call: SetSystemCursor takes ownership and destroys what
        // it is given, so handing it the stored handle would leave the
        // second toggle with a dangling one.
        // SAFETY: the stored handles come from CreateIconIndirect below and
        // are alive for the life of the process.
        let Ok(copy) = (unsafe { CopyIcon(HICON(*cursor as *mut _)) }) else {
            continue;
        };
        // SAFETY: `copy` is a live cursor of the system's own size; `id` is
        // one of the documented OCR_* constants. Ownership passes here.
        let _ = unsafe { SetSystemCursor(HCURSOR(copy.0), *id) };
    }
    INSTALLED.store(true, Ordering::Relaxed);
}

/// Makes the ends that still run code put the cursor back: a Rust panic and
/// an unhandled Win32 exception (ADR 0067 decision 3).
///
/// A hard kill runs nothing, by design — that case is covered by the tint
/// only ever meaning "IME is on", so a cursor left behind is legible as
/// "WinRemap died", and by [`restore`] at the next start.
pub fn install_crash_restore() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
    // SAFETY: the filter is a plain `extern "system"` function with no
    // state; passing it is the documented way to be told about an unhandled
    // exception before the process dies.
    unsafe { SetUnhandledExceptionFilter(Some(on_unhandled_exception)) };
}

/// Runs on the failing thread with the process about to die: restore, then
/// let the previous filter (the CRT's, which reports the crash) run.
unsafe extern "system" fn on_unhandled_exception(_info: *const EXCEPTION_POINTERS) -> i32 {
    if INSTALLED.load(Ordering::Relaxed) {
        restore();
    }
    // EXCEPTION_CONTINUE_SEARCH: this is a cleanup hook, not a handler.
    0
}

/// Builds a recoloured copy of one of the system's cursors.
///
/// Both kinds of source are handled: modern cursors carry a 32-bit colour
/// bitmap with alpha, while the classic I-beam is a mask-only cursor whose
/// bitmap is twice as tall (AND rows over XOR rows). Reading either through
/// `GetDIBits` as 32-bit makes them the same problem.
fn tinted(name: PCWSTR, color: (u8, u8, u8)) -> Option<HCURSOR> {
    // LR_SHARED: this handle belongs to the system and must not be
    // destroyed. LR_DEFAULTSIZE gives the size the system cursor is at, so
    // the replacement matches on a high-DPI display.
    // SAFETY: `name` is one of the IDC_* atoms; a null instance means the
    // predefined cursors.
    let raw: HANDLE =
        unsafe { LoadImageW(None, name, IMAGE_CURSOR, 0, 0, LR_SHARED | LR_DEFAULTSIZE) }.ok()?;
    let mut info = ICONINFO::default();
    // SAFETY: `raw` is a live cursor handle; `info` is a live local.
    unsafe { GetIconInfo(HICON(raw.0), &mut info) }.ok()?;
    // GetIconInfo hands out bitmap copies that the caller owns.
    let mask = Bitmap(info.hbmMask);
    let color_bmp = Bitmap(info.hbmColor);

    let (width, height, pixels) = if color_bmp.0.is_invalid() {
        from_mask_only(mask.0, color)?
    } else {
        from_color(color_bmp.0, mask.0, color)?
    };

    // Transparency rides in the alpha channel, so the AND mask says "take the
    // colour bitmap" everywhere: all zeros. Passed explicitly — CreateBitmap
    // leaves the contents *undefined* when given no bits, and an undefined
    // AND mask is a cursor with holes in it wherever the memory happened to
    // be set.
    let blank = vec![0u8; (((width + 15) / 16) * 2 * height) as usize];
    // SAFETY: a 1bpp monochrome bitmap; `blank` is sized to its stride
    // (rows padded to a 16-bit boundary) times its height.
    let empty_mask = Bitmap(unsafe {
        CreateBitmap(
            width,
            height,
            1,
            1,
            Some(blank.as_ptr() as *const core::ffi::c_void),
        )
    });
    let colored = Bitmap(dib_section(width, height, &pixels)?);
    let icon_info = ICONINFO {
        fIcon: false.into(),
        xHotspot: info.xHotspot,
        yHotspot: info.yHotspot,
        hbmMask: empty_mask.0,
        hbmColor: colored.0,
    };
    // SAFETY: both bitmaps are live and of the same size; fIcon = false asks
    // for a cursor, which is what the hotspots are for.
    let icon = unsafe { CreateIconIndirect(&icon_info) }.ok()?;
    Some(HCURSOR(icon.0))
}

/// Copies 32-bit BGRA pixels into a bitmap a cursor can be built from.
///
/// A DIB section, not `CreateBitmap`: `CreateIconIndirect` only honours the
/// alpha channel of a **device-independent** 32-bit bitmap. Given a
/// device-dependent one it ignores alpha and falls back to the AND mask,
/// which here says "opaque everywhere" — and the cursor comes out as a solid
/// rectangle. That is what an I-beam looked like on the first attempt: a
/// black square (owner report, 2026-08-02).
fn dib_section(width: i32, height: i32, pixels: &[u32]) -> Option<HBITMAP> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative: top-down, matching the order `pixels` is in.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: `info` describes the buffer the call hands back through `bits`;
    // a null DC is allowed for DIB_RGB_COLORS.
    let bitmap =
        unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()?;
    if bits.is_null() {
        // SAFETY: the bitmap was just created here and is not used again.
        let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
        return None;
    }
    // SAFETY: the section is width*height 32-bit pixels, exactly the length
    // of `pixels`, and nothing else refers to it yet.
    unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u32, pixels.len()) };
    Some(bitmap)
}

/// A modern cursor: colour bitmap plus, for the ones without alpha, the AND
/// mask that says which pixels are transparent.
fn from_color(
    color_bmp: HBITMAP,
    mask: HBITMAP,
    color: (u8, u8, u8),
) -> Option<(i32, i32, Vec<u32>)> {
    let (width, height, source) = read_as_bgra(color_bmp)?;
    let has_alpha = source.iter().any(|px| px >> 24 != 0);
    let and_mask = if has_alpha {
        None
    } else {
        read_as_bgra(mask).map(|(_, _, bits)| bits)
    };
    let pixels = source
        .iter()
        .enumerate()
        .map(|(i, px)| {
            let opaque = match &and_mask {
                // The AND mask is 1 where the background shows through.
                Some(bits) => bits.get(i).is_some_and(|m| m & 0x00FF_FFFF == 0),
                None => px >> 24 != 0,
            };
            if !opaque {
                return 0;
            }
            let alpha = if has_alpha { px >> 24 } else { 0xFF };
            tint(*px, color) | (alpha << 24)
        })
        .collect();
    Some((width, height, pixels))
}

/// A classic mask-only cursor (the default I-beam is one): the bitmap is
/// twice as tall, the AND rows above the XOR rows. The two bits together pick
/// one of four things to do with the pixel underneath:
///
/// | AND | XOR | classic meaning | here |
/// |---|---|---|---|
/// | 1 | 0 | leave the screen alone | transparent |
/// | 1 | 1 | **invert** the screen | the colour |
/// | 0 | 0 | black | black |
/// | 0 | 1 | white | the colour |
///
/// The invert row is not a corner case: **the stock I-beam is drawn entirely
/// out of it** (measured 2026-08-02 — its AND plane is 1 everywhere, and all
/// 26 visible pixels are AND=1/XOR=1). That is how it stays legible on a dark
/// background as well as a light one. Dropping those pixels as "no colour to
/// tint" left a cursor with nothing opaque in it at all, and Windows reads a
/// 32-bit bitmap whose alpha is uniformly zero as having no alpha channel: it
/// falls back to the AND mask, which here is opaque everywhere, and draws the
/// all-black colour bitmap as a **solid black square** (owner report,
/// 2026-08-02).
///
/// Painting them the tint colour loses the inversion — on a background of
/// that same colour the I-beam is harder to pick out than the stock one. That
/// is the trade this feature already makes everywhere else: the point is that
/// the colour says the IME is on.
fn from_mask_only(mask: HBITMAP, color: (u8, u8, u8)) -> Option<(i32, i32, Vec<u32>)> {
    let (width, double_height, bits) = read_as_bgra(mask)?;
    let height = double_height / 2;
    if height == 0 {
        return None;
    }
    let plane = (width * height) as usize;
    let pixels = (0..plane)
        .map(|i| {
            let and = bits[i] & 0x00FF_FFFF != 0;
            let lit = bits[plane + i] & 0x00FF_FFFF != 0;
            match (and, lit) {
                (true, false) => 0,
                (_, true) => 0xFF00_0000 | tint(0x00FF_FFFF, color),
                (false, false) => 0xFF00_0000,
            }
        })
        .collect();
    Some((width, height, pixels))
}

/// Recolours one pixel: its brightness decides how much of `color` it gets.
/// Black outlines stay black, a white body becomes the colour, and the
/// anti-aliased edge in between keeps its shading.
fn tint(bgra: u32, (r, g, b): (u8, u8, u8)) -> u32 {
    let (sb, sg, sr) = (bgra & 0xFF, (bgra >> 8) & 0xFF, (bgra >> 16) & 0xFF);
    // Rec. 601 luma, the same weights the eye applies.
    let luma = (sr * 30 + sg * 59 + sb * 11) / 100;
    let scale = |c: u8| ((u32::from(c) * luma) / 255) & 0xFF;
    (scale(r) << 16) | (scale(g) << 8) | scale(b)
}

/// Reads any bitmap as top-down 32-bit BGRA, whatever it is stored as.
/// Monochrome sources come back as black and white, which is what makes the
/// mask-only path above simple.
fn read_as_bgra(bitmap: HBITMAP) -> Option<(i32, i32, Vec<u32>)> {
    let mut header = BITMAP::default();
    // SAFETY: `header` is a live local of exactly the size passed.
    let read = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0),
            size_of::<BITMAP>() as i32,
            Some(&mut header as *mut BITMAP as *mut core::ffi::c_void),
        )
    };
    if read == 0 || header.bmWidth <= 0 || header.bmHeight <= 0 {
        return None;
    }
    let (width, height) = (header.bmWidth, header.bmHeight);
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative: top-down, so row 0 is the top one and the mask-only
            // path can say "the AND rows come first".
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u32; (width * height) as usize];
    // SAFETY: a null DC is allowed for GetDIBits; the buffer is sized from
    // the header the call is given.
    let dc = unsafe { CreateCompatibleDC(None) };
    let lines = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut core::ffi::c_void),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    // SAFETY: `dc` was just created here and is not used again.
    let _ = unsafe { DeleteDC(dc) };
    (lines != 0).then_some((width, height, pixels))
}

/// Deletes a GDI bitmap on the way out. GetIconIndirect and GetIconInfo both
/// hand out bitmaps the caller has to free, and this feature builds four of
/// them per cursor.
struct Bitmap(HBITMAP);

impl Drop for Bitmap {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: the handle is ours and is not referenced afterwards —
            // CreateIconIndirect copies what it is given.
            let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
        }
    }
}
