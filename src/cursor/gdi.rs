//! GDI bitmaps: reading a cursor's planes out as BGRA and writing a
//! recoloured one back into a DIB section.

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDIBits, GetObjectW, HBITMAP, HGDIOBJ,
};

use super::pixels::tint;

/// Copies 32-bit BGRA pixels into a bitmap a cursor can be built from.
///
/// A DIB section, not `CreateBitmap`: `CreateIconIndirect` only honours the
/// alpha channel of a **device-independent** 32-bit bitmap. Given a
/// device-dependent one it ignores alpha and falls back to the AND mask,
/// which here says "opaque everywhere" — and the cursor comes out as a solid
/// rectangle. That is what an I-beam looked like on the first attempt: a
/// black square (owner report, 2026-08-02).
pub(super) fn dib_section(width: i32, height: i32, pixels: &[u32]) -> Option<HBITMAP> {
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
pub(super) fn from_color(
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
pub(super) fn from_mask_only(mask: HBITMAP, color: (u8, u8, u8)) -> Option<(i32, i32, Vec<u32>)> {
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
pub(super) struct Bitmap(pub(super) HBITMAP);

impl Drop for Bitmap {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: the handle is ours and is not referenced afterwards —
            // CreateIconIndirect copies what it is given.
            let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
        }
    }
}
