//! Reading one of the system's cursors and building its recoloured copy.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::CreateBitmap;
use windows::Win32::UI::WindowsAndMessaging::{
    CopyIcon, CreateIconIndirect, GetIconInfo, HCURSOR, HICON, ICONINFO, IMAGE_CURSOR,
    LR_DEFAULTSIZE, LR_SHARED, LoadImageW, SYSTEM_CURSOR_ID,
};
use windows::core::PCWSTR;

use super::Trouble;
use super::gdi::{Bitmap, dib_section, from_color, from_mask_only};
use super::pixels::{and_mask, outlined};
use super::scale::{NOTHING_DRAWN, unscaled_retry};

/// Asks the system for one of its own cursors.
///
/// No module handle: that is the spelling that works (measured — the
/// user32-relative one is the `1814` above). LR_SHARED hands back the
/// system's own handle, which must not be destroyed, and is also required for
/// a system cursor — without it the call fails. LR_DEFAULTSIZE gives the size
/// this display is using, which is the part [`unscaled_retry`] second-guesses.
pub(super) fn load(name: PCWSTR) -> Result<HICON, &'static str> {
    // SAFETY: `name` is one of the IDC_* atoms; a null instance means the
    // predefined cursors.
    let raw: HANDLE =
        unsafe { LoadImageW(None, name, IMAGE_CURSOR, 0, 0, LR_SHARED | LR_DEFAULTSIZE) }
            .map_err(|_| "LoadImageW")?;
    Ok(HICON(raw.0))
}

/// One pristine copy, or why there is not one.
pub(super) fn snapshot(name: PCWSTR) -> Result<HICON, &'static str> {
    unscaled_retry(|why| *why == NOTHING_DRAWN, || snapshot_at_this_scale(name))
}

fn snapshot_at_this_scale(name: PCWSTR) -> Result<HICON, &'static str> {
    let raw = load(name)?;
    match drawn_pixels(raw) {
        None => return Err("the cursor could not be read"),
        Some(0) => return Err(NOTHING_DRAWN),
        Some(_) => {}
    }
    // SAFETY: a live cursor handle; the copy is ours to keep for the life of
    // the process, and the shared original is left alone.
    unsafe { CopyIcon(raw) }.map_err(|_| "CopyIcon")
}

/// One of the system's cursors read out as pixels this module can work on.
struct Decoded {
    width: i32,
    height: i32,
    /// Top-down BGRA, already recoloured. **A zero alpha means nothing is
    /// drawn there** — which is what makes counting emptiness the same job
    /// for both kinds of source.
    pixels: Vec<u32>,
    hotspot: (u32, u32),
}

/// Reads a cursor and recolours it, whichever of the two shapes it has:
/// modern cursors carry a 32-bit colour bitmap with alpha, while the classic
/// I-beam is a mask-only cursor whose bitmap is twice as tall (AND rows over
/// XOR rows). Reading either through `GetDIBits` as 32-bit makes them the
/// same problem.
fn decode(icon: HICON, color: (u8, u8, u8)) -> Option<Decoded> {
    let mut info = ICONINFO::default();
    // SAFETY: `icon` is a live cursor handle; `info` is a live local.
    unsafe { GetIconInfo(icon, &mut info) }.ok()?;
    // GetIconInfo hands out bitmap copies that the caller owns.
    let mask = Bitmap(info.hbmMask);
    let color_bmp = Bitmap(info.hbmColor);

    let (width, height, pixels) = if color_bmp.0.is_invalid() {
        from_mask_only(mask.0, color)?
    } else {
        from_color(color_bmp.0, mask.0, color)?
    };
    Some(Decoded {
        width,
        height,
        pixels,
        hotspot: (info.xHotspot, info.yHotspot),
    })
}

/// How many pixels of a cursor would actually be drawn — `None` if it could
/// not be read at all.
///
/// **Zero is the whole of M-2 in one number.** What the owner saw four times
/// was a cursor Windows was perfectly happy to install and draw nothing of.
/// Nothing measured it, on either side: not the tint on its way in, not the
/// cursor it was made from (ADR 0073 decisions 3 and 4).
pub(super) fn drawn_pixels(icon: HICON) -> Option<usize> {
    // White because it has to be something; the count is of alpha, which
    // recolouring never touches.
    let decoded = decode(icon, (0xFF, 0xFF, 0xFF))?;
    Some(decoded.pixels.iter().filter(|px| *px >> 24 != 0).count())
}

/// Builds a recoloured copy of one of the system's cursors, or says why it
/// could not (ADR 0073 decision 5 — this used to be five silent `?`s).
pub(super) fn tinted(
    id: SYSTEM_CURSOR_ID,
    name: PCWSTR,
    color: (u8, u8, u8),
) -> Result<HCURSOR, Trouble> {
    // An empty source is retried unscaled (ADR 0076); anything else it says
    // is final. `SourceEmpty` therefore now means what it says — the cursor
    // is empty however it is asked for, not merely at this display's scale.
    unscaled_retry(
        |trouble| matches!(trouble, Trouble::SourceEmpty { .. }),
        || tinted_at_this_scale(id, name, color),
    )
}

fn tinted_at_this_scale(
    id: SYSTEM_CURSOR_ID,
    name: PCWSTR,
    color: (u8, u8, u8),
) -> Result<HCURSOR, Trouble> {
    let raw = match load(name) {
        Ok(raw) => raw,
        Err(why) => return Err(build_failed(id, why)),
    };
    let Some(decoded) = decode(raw, color) else {
        return Err(build_failed(id, "the cursor could not be read"));
    };
    let Decoded {
        width,
        height,
        pixels,
        hotspot,
    } = decoded;
    // Decision 4: an empty source produces an empty tint, and `from_color`
    // falls back to the AND mask when it finds no alpha — so an empty one
    // feeds itself. This is also what `unscaled_retry` watches for, so on a
    // scaled display it is the trigger for reading the cursor again unscaled
    // rather than the end of the road (ADR 0076).
    if pixels.iter().all(|px| px >> 24 == 0) {
        return Err(Trouble::SourceEmpty { id });
    }

    let pixels = outlined(width, height, &pixels);
    // Decision 3: and this is where an empty *result* stops, whatever made
    // it. `outlined` can produce one from a source that is not empty — a
    // cursor whose every pixel is less than half opaque has no solid shape
    // to keep and no solid neighbour to draw a border around.
    if pixels.iter().all(|px| px >> 24 == 0) {
        return Err(Trouble::ResultEmpty { id });
    }
    let bits = and_mask(width, height, &pixels);
    // SAFETY: a 1bpp monochrome bitmap; `bits` is sized to its stride (rows
    // padded to a 16-bit boundary) times its height.
    let shape = Bitmap(unsafe {
        CreateBitmap(
            width,
            height,
            1,
            1,
            Some(bits.as_ptr() as *const core::ffi::c_void),
        )
    });
    let Some(section) = dib_section(width, height, &pixels) else {
        return Err(build_failed(id, "CreateDIBSection"));
    };
    let colored = Bitmap(section);
    let icon_info = ICONINFO {
        fIcon: false.into(),
        xHotspot: hotspot.0,
        yHotspot: hotspot.1,
        hbmMask: shape.0,
        hbmColor: colored.0,
    };
    // SAFETY: both bitmaps are live and of the same size; fIcon = false asks
    // for a cursor, which is what the hotspots are for.
    match unsafe { CreateIconIndirect(&icon_info) } {
        Ok(icon) => Ok(HCURSOR(icon.0)),
        Err(_) => Err(build_failed(id, "CreateIconIndirect")),
    }
}

/// Names the Win32 call that would not do its part.
///
/// Only the failures with a call to name go through here; an empty source or
/// an empty result are their own [`Trouble`] and say so themselves.
fn build_failed(id: SYSTEM_CURSOR_ID, why: &'static str) -> Trouble {
    Trouble::BuildFailed { id, why }
}
