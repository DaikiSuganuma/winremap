//! The arithmetic of the tint: recolouring, the white border, and the AND
//! mask. No Win32 here — these are the parts a unit test can hold still.

/// Puts a white border around the cursor's solid shape, and drops whatever
/// was outside it.
///
/// **Without this the tint is invisible on a dark application** (owner
/// report, 2026-08-02). The reason is arithmetic: the tint maps the cursor's
/// white body to the chosen colour, and `#0078d4` is about 37% as bright as
/// white. What used to be the brightest thing on the screen becomes a middling
/// one — fine on a white page, lost on a black editor.
///
/// A border in the opposite direction fixes it by construction: the cursor is
/// then made of two colours at opposite ends of the brightness range, so
/// **whatever the background, one of them stands out**. Windows takes the same
/// approach for its own custom-coloured pointer.
///
/// Synthesised rather than recoloured, because there is often nothing to
/// recolour. The stock arrow on Windows 11 has **no black outline at all** —
/// measured 2026-08-02: 77 opaque white pixels and 67 semi-transparent ones
/// making a soft drop shadow, and not one opaque black pixel. The I-beam has
/// no outline either. So the border is drawn from the shape itself: solid
/// pixels keep the tint, their transparent neighbours become opaque white,
/// and the rest — the old shadow, which a white border makes redundant —
/// goes away.
pub(super) fn outlined(width: i32, height: i32, pixels: &[u32]) -> Vec<u32> {
    let (w, h) = (width as usize, height as usize);
    // Half-transparent is the dividing line: the anti-aliased fringe of the
    // body belongs to the border, not to the shape.
    let solid: Vec<bool> = pixels.iter().map(|px| px >> 24 >= 128).collect();
    let mut out = vec![0u32; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if solid[i] {
                out[i] = pixels[i];
                continue;
            }
            let touches = (y.saturating_sub(1)..=(y + 1).min(h - 1))
                .any(|ny| (x.saturating_sub(1)..=(x + 1).min(w - 1)).any(|nx| solid[ny * w + nx]));
            if touches {
                out[i] = 0xFFFF_FFFF;
            }
        }
    }
    out
}

/// The 1-bit AND mask that says where the screen shows through: 1 for the
/// pixels whose alpha is zero, 0 for the rest.
///
/// **Not redundant with the alpha channel — it is the fallback for whoever
/// ignores it.** Windows has more than one path for putting a cursor on the
/// screen, and a 32-bit colour bitmap with an all-zero mask is only right on
/// the paths that read alpha; the others take the mask at its word and draw a
/// **solid rectangle**. Zed showed exactly that (owner report, 2026-08-02),
/// and the earlier "the alpha was uniformly zero" bug was the same failure
/// from the other end. Filling the mask in means the shape survives either
/// way — the alpha path additionally keeps the anti-aliased edge.
///
/// Rows are padded to a 16-bit boundary and the leftmost pixel is the high
/// bit of the first byte, which is what `CreateBitmap` expects of a
/// monochrome bitmap.
pub(super) fn and_mask(width: i32, height: i32, pixels: &[u32]) -> Vec<u8> {
    let stride = (((width + 15) / 16) * 2) as usize;
    let mut bits = vec![0u8; stride * height as usize];
    for y in 0..height as usize {
        for x in 0..width as usize {
            if pixels[y * width as usize + x] >> 24 == 0 {
                bits[y * stride + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    bits
}

/// Recolours one pixel: its brightness decides how much of `color` it gets.
/// Black outlines stay black, a white body becomes the colour, and the
/// anti-aliased edge in between keeps its shading.
pub(super) fn tint(bgra: u32, (r, g, b): (u8, u8, u8)) -> u32 {
    let (sb, sg, sr) = (bgra & 0xFF, (bgra >> 8) & 0xFF, (bgra >> 16) & 0xFF);
    // Rec. 601 luma, the same weights the eye applies.
    let luma = (sr * 30 + sg * 59 + sb * 11) / 100;
    let scale = |c: u8| ((u32::from(c) * luma) / 255) & 0xFF;
    (scale(r) << 16) | (scale(g) << 8) | scale(b)
}
