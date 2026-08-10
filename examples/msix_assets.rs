//! Rasterizes the MSIX package assets from the master SVG (ADR 0060).
//!
//! Run when `assets/svg/kbd-enabled.svg` changes; the PNGs it writes are
//! committed, so packaging needs no toolchain beyond the Windows SDK:
//!
//! ```text
//! cargo run --example msix_assets
//! ```
//!
//! Rendering from the SVG rather than downscaling `assets/png/*-256.png`
//! matters at the large end: a 150x150 tile at scale-400 is 600px, and no
//! amount of resampling invents detail a 256px source never had.

use std::path::{Path, PathBuf};

/// Master artwork. The tray icons come from the same file (ADR 0010), which
/// is what keeps the Store listing and the running app showing one icon.
const SOURCE: &str = "assets/svg/kbd-enabled.svg";
const OUT_DIR: &str = "packaging/msix/Assets";

/// The three logos MSIX asks for, with the scale factors Windows picks
/// between by display DPI. The unqualified base size is written too: without
/// a `resources.pri`, that is the only name the loader resolves, and a
/// sideload test should not need MakePri to show an icon.
const LOGOS: &[(&str, u32)] = &[
    ("Square44x44Logo", 44),
    ("Square150x150Logo", 150),
    ("StoreLogo", 50),
];

const SCALES: &[u32] = &[100, 125, 150, 200, 400];

/// Start menu, taskbar and Alt+Tab ask the 44x44 logo for exact pixel sizes
/// instead of a scale factor. Without these, Windows downsamples the 44px
/// bitmap and the keycaps blur together.
const TARGET_SIZES: &[u32] = &[16, 24, 32, 48, 256];

/// The same sizes again, under the name the *unplated* surfaces look for.
///
/// Settings → Apps → Startup, the taskbar and a few other places draw the icon
/// without a backplate and ask for `_altform-unplated`. **When it is missing
/// they do not fall back to the plain file — they fall back to the plated
/// rendering**, which is the logo drawn on top of a solid plate. This package
/// sets `BackgroundColor="transparent"`, so that plate is the user's accent
/// colour, and a blue logo on a blue plate reads as a blue square with nothing
/// in it (v0.8 acceptance P-4, owner report 2026-08-09). Same pixels as the
/// plated file; only the name is doing the work.
const UNPLATED: &str = "_altform-unplated";

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let svg = std::fs::read_to_string(root.join(SOURCE))
        .unwrap_or_else(|e| panic!("failed to read {SOURCE}: {e}"));
    let out = root.join(OUT_DIR);
    std::fs::create_dir_all(&out).expect("failed to create the asset directory");

    let mut count = 0;
    for (name, base) in LOGOS {
        write_png(&svg, &out.join(format!("{name}.png")), *base);
        count += 1;
        for scale in SCALES {
            // Round rather than truncate: 150 at 125% is 187.5, and Windows
            // expects 188.
            let size = (*base as f64 * f64::from(*scale) / 100.0).round() as u32;
            write_png(&svg, &out.join(format!("{name}.scale-{scale}.png")), size);
            count += 1;
        }
    }
    for size in TARGET_SIZES {
        let path = out.join(format!("Square44x44Logo.targetsize-{size}.png"));
        write_png(&svg, &path, *size);
        count += 1;
        let unplated = out.join(format!("Square44x44Logo.targetsize-{size}{UNPLATED}.png"));
        write_png(&svg, &unplated, *size);
        count += 1;
    }

    println!("wrote {count} assets to {OUT_DIR}");
}

fn write_png(svg: &str, path: &Path, size: u32) {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())
        .unwrap_or_else(|e| panic!("failed to parse {SOURCE}: {e}"));
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(size, size).expect("an icon-sized pixmap is allocatable");
    let scale = size as f32 / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let png = pixmap.encode_png().expect("pixmap encodes as PNG");
    std::fs::write(path, png).unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
}
