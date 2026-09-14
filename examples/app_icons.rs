//! Regenerates the app icons from the master SVGs (ADR 0010, ADR 0081, ADR 0082).
//!
//! ADR 0010 made `assets/svg/` the master and said to regenerate the PNGs and
//! the .ico files when it changes — but nothing did the regenerating, so the
//! committed files were hand-made in an external tool. Redrawing the icon then
//! means rebuilding every size of two faces plus two .ico containers by hand,
//! which is exactly the kind of step that gets half-done. This is that tool:
//!
//! ```text
//! cargo run --example app_icons
//! ```
//!
//! Writes `assets/png/kbd-{enabled,disabled}-<size>.png` and `assets/kbd.ico` /
//! `assets/kbd-disabled.ico`. Run `msix_assets` as well when a face changes —
//! that one feeds the Store listing, and the two must not drift apart.
//!
//! How to draw the masters, and why the small ones exist:
//! `docs/06_icon-assets.md`.

use std::path::{Path, PathBuf};

/// Which master draws a given size (ADR 0082).
#[derive(PartialEq, Clone, Copy)]
enum Master {
    /// The simplified face, for sizes where the detailed one turns to mush.
    Small,
    /// The full artwork.
    Main,
}

/// Every size the shell asks an exe for, and the master that draws it.
///
/// The notification area asks for `SM_CXSMICON` at the monitor's scale — 16 at
/// 100%, 20 at 125%, 24 at 150%, 28 at 175%, 32 at 200% — and the large-icon
/// surfaces (Explorer, Alt+Tab, a window's `ICON_BIG`) ask for twice that. **A
/// size the .ico does not carry gets scaled by Windows**, which is the blur
/// ADR 0080 was about, so the tray's whole range is present rather than the
/// 16/24/32 the hand-made files happened to have. 56 and 64 are left out: the
/// large-icon surfaces are far less sensitive than a 24 px tray slot, and every
/// frame is uncompressed (64x64 alone would add 17 KB).
///
/// The split at 32 is where the tray stops asking. Drawing the tray's whole
/// range from one master is the point — otherwise the icon would change shape
/// as the user moved the window between monitors of different scale.
const SIZES: &[(u32, Master)] = &[
    (16, Master::Small),
    (20, Master::Small),
    (24, Master::Small),
    (28, Master::Small),
    (32, Master::Small),
    (40, Master::Main),
    (48, Master::Main),
    (256, Master::Main),
];

/// Above this the .ico frame is stored as PNG rather than an uncompressed
/// DIB, which is how the previous hand-made files were laid out: a 256x256
/// DIB alone would be 256 KB in a 20 KB file.
const PNG_FRAME_AT_OR_ABOVE: u32 = 256;

struct Face {
    main_svg: &'static str,
    /// Optional on purpose: until it is drawn, the main master covers the
    /// small sizes too, so the pipeline works before the artwork lands and
    /// says which master it actually used.
    small_svg: &'static str,
    png_prefix: &'static str,
    ico: &'static str,
}

const FACES: &[Face] = &[
    Face {
        main_svg: "assets/svg/kbd-enabled.svg",
        small_svg: "assets/svg/kbd-enabled-small.svg",
        png_prefix: "kbd-enabled",
        ico: "assets/kbd.ico",
    },
    Face {
        main_svg: "assets/svg/kbd-disabled.svg",
        small_svg: "assets/svg/kbd-disabled-small.svg",
        png_prefix: "kbd-disabled",
        ico: "assets/kbd-disabled.ico",
    },
];

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for face in FACES {
        let main = read(&root.join(face.main_svg), face.main_svg);
        let small_path = root.join(face.small_svg);
        let small = small_path
            .exists()
            .then(|| read(&small_path, face.small_svg));
        if small.is_none() {
            println!(
                "  ({} が無いので小サイズも通常マスターから焼く)",
                face.small_svg
            );
        }

        let mut frames = Vec::new();
        for (size, master) in SIZES {
            let (svg, source) = match (master, &small) {
                (Master::Small, Some(svg)) => (svg, face.small_svg),
                _ => (&main, face.main_svg),
            };
            let pixmap = render(svg, *size, source);
            let png = pixmap.encode_png().expect("pixmap encodes as PNG");

            let path = root
                .join("assets/png")
                .join(format!("{}-{size}.png", face.png_prefix));
            write(&path, &png);

            frames.push((
                *size,
                if *size >= PNG_FRAME_AT_OR_ABOVE {
                    png
                } else {
                    dib_frame(&pixmap)
                },
            ));
        }
        write(&root.join(face.ico), &ico(&frames));
    }
    println!(
        "regenerated {} PNGs and {} .ico files",
        FACES.len() * SIZES.len(),
        FACES.len()
    );
}

fn read(path: &Path, name: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {name}: {e}"))
}

/// One SVG into a square pixmap. The artwork is wider than it is tall, so it
/// is the *viewBox* that is square — scaling by width keeps the drawing
/// centred with the padding the master already has.
///
/// Both checks below are Illustrator export mistakes that produce a plausible
/// image rather than an error, so they would otherwise reach the exe
/// unnoticed (2026-09-14, the first masters exported from Illustrator had one
/// each). Failing here is deliberate: an icon quietly drawn in the wrong place
/// is the kind of thing nobody notices for a release.
fn render(svg: &str, size: u32, source: &str) -> resvg::tiny_skia::Pixmap {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())
        .unwrap_or_else(|e| panic!("failed to parse {source}: {e}"));

    // Saved without "use artboards", Illustrator crops the viewBox to the
    // artwork's bounds, and the padding is gone: a 14x10 body scaled to the
    // square's width lands flush against the top.
    let (w, h) = (tree.size().width(), tree.size().height());
    if w != h {
        panic!(
            "{source} の viewBox が {w}x{h} で正方形ではない。Illustrator が絵の外接矩形で切り抜いて\
             書き出している — アートボード（16x16）で書き出し直すこと（docs/06_icon-assets.md §5.7）"
        );
    }

    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(size, size).expect("an icon-sized pixmap is allocatable");
    let scale = size as f32 / w;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    // The corners are outside the rounded body in every face this app has.
    // Paint there means a background rectangle was drawn to force the bounds
    // to 16x16 — and it would be baked into the icon, a white square on a
    // dark taskbar.
    let last = size - 1;
    for (x, y) in [(0, 0), (last, 0), (0, last), (last, last)] {
        if pixmap.pixel(x, y).is_some_and(|p| p.alpha() != 0) {
            panic!(
                "{source} を {size}px で焼くと四隅が塗られている。背景の四角を入れていないか — \
                 アイコンの周りは透明でなければならない（docs/06_icon-assets.md §5.4）"
            );
        }
    }
    pixmap
}

/// A 32-bit BI_RGB frame: BITMAPINFOHEADER, then the colour rows bottom-up as
/// BGRA, then the 1-bit AND mask.
///
/// The mask is all zeros. It predates alpha and every renderer that matters
/// reads the alpha channel instead, but the bytes still have to be there —
/// the header claims a height of twice the image for exactly this reason.
fn dib_frame(pixmap: &resvg::tiny_skia::Pixmap) -> Vec<u8> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mask_stride = w.div_ceil(32) * 4;
    let mut out = Vec::with_capacity((40 + w * h * 4 + mask_stride * h) as usize);

    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&((h * 2) as i32).to_le_bytes()); // colour rows + mask
    out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    out.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    out.extend_from_slice(&(w * h * 4 + mask_stride * h).to_le_bytes()); // biSizeImage
    out.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
    out.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

    // tiny-skia stores premultiplied pixels; an .ico frame wants straight
    // ones, or every semi-transparent edge darkens.
    let pixels = pixmap.pixels();
    for y in (0..h).rev() {
        for x in 0..w {
            let c = pixels[(y * w + x) as usize].demultiply();
            out.extend_from_slice(&[c.blue(), c.green(), c.red(), c.alpha()]);
        }
    }
    out.resize(out.len() + (mask_stride * h) as usize, 0);
    out
}

/// The ICONDIR container: header, one 16-byte entry per frame, then the
/// frames themselves.
fn ico(frames: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&(frames.len() as u16).to_le_bytes());

    let mut offset = 6 + 16 * frames.len() as u32;
    for (size, data) in frames {
        // 256 is written as 0: the field is one byte, and 256 does not fit.
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bit count
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in frames {
        out.extend_from_slice(data);
    }
    out
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
    println!("  {} ({} bytes)", path.display(), bytes.len());
}
