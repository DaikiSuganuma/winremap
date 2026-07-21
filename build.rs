//! Build-time asset preparation.
//!
//! Two jobs, neither of which costs anything at runtime:
//!
//! * Embeds the tray/app icons as Windows resources (ADR 0010). Ordinal 1 is
//!   also picked up by Explorer as the executable's icon.
//! * Rasterizes the tray menu's Bootstrap Icons from SVG to RGBA (ADR 0040).
//!   Win32 menus take raw pixels, and egui cannot draw SVG, so the conversion
//!   has to happen somewhere; doing it here keeps `resvg` out of the shipped
//!   binary — only the pixels it produces are embedded.

use std::path::{Path, PathBuf};

/// Menu icons are drawn at the classic 16x16 menu-bitmap size.
const ICON_SIZE: u32 = 16;

/// Bootstrap Icons ship with `fill="currentColor"`, which has no meaning
/// outside a document; this is the colour they are baked with. Dark, because
/// Win32 popup menus render on the light system menu background unless an app
/// opts into dark mode, which WinRemap does not.
const ICON_COLOR: &str = "#343a40";

/// SVGs from https://github.com/twbs/icons (MIT), vendored under assets/icons.
const MENU_ICONS: &[&str] = &["gear", "arrow-clockwise", "card-list", "box-arrow-right"];

fn main() {
    println!("cargo:rerun-if-changed=assets/kbd.ico");
    println!("cargo:rerun-if-changed=assets/kbd-disabled.ico");
    // CARGO_CFG_WINDOWS guards the resource compiler for any future
    // cross-compilation from non-Windows hosts.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        winresource::WindowsResource::new()
            .set_icon_with_id("assets/kbd.ico", "1")
            .set_icon_with_id("assets/kbd-disabled.ico", "2")
            .compile()
            .expect("failed to embed icon resources");
    }

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by cargo"));
    for name in MENU_ICONS {
        let source = Path::new("assets/icons").join(format!("{name}.svg"));
        println!("cargo:rerun-if-changed={}", source.display());
        let rgba = rasterize(&source);
        std::fs::write(out_dir.join(format!("{name}.rgba")), rgba)
            .unwrap_or_else(|e| panic!("failed to write the rasterized {name}: {e}"));
    }
}

/// One SVG to straight (non-premultiplied) RGBA, which is what muda wants. A
/// failure is a build failure on purpose: a menu quietly losing its icons is
/// the kind of thing nobody notices for a release or two.
fn rasterize(source: &Path) -> Vec<u8> {
    let svg = std::fs::read_to_string(source)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", source.display()));
    let svg = svg.replace("currentColor", ICON_COLOR);

    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", source.display()));
    let mut pixmap = resvg::tiny_skia::Pixmap::new(ICON_SIZE, ICON_SIZE)
        .expect("a 16x16 pixmap is always allocatable");
    let scale = ICON_SIZE as f32 / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    // tiny-skia stores premultiplied alpha; consumers of raw RGBA expect
    // straight alpha, and anti-aliased edges look wrong without this.
    pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect()
}
