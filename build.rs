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

/// Bootstrap Icons ship with `fill="currentColor"`, which has no meaning
/// outside a document, so a colour is baked in at rasterization time.
///
/// The tray menu is stuck with a fixed one: Win32 takes finished pixels and
/// draws them on the light system menu background, since WinRemap does not opt
/// into dark mode. Window icons are baked white instead — egui can tint them,
/// so one white face follows the light and dark themes both.
const MENU_ICON_COLOR: &str = "#343a40";
const UI_ICON_COLOR: &str = "#ffffff";

/// Menu icons are drawn at the classic 16x16 menu-bitmap size. Window icons
/// are drawn at 16 logical points but rasterized at twice that, so they stay
/// sharp on a HiDPI display.
const MENU_ICON_SIZE: u32 = 16;
const UI_ICON_SIZE: u32 = 32;

/// SVGs from https://github.com/twbs/icons (MIT), vendored under assets/icons.
/// See THIRD-PARTY-NOTICES.md — the pixels these become ship in the binary.
const MENU_ICONS: &[&str] = &["gear", "arrow-clockwise", "card-list", "box-arrow-right"];

/// The settings window's section headings and links.
const UI_ICONS: &[&str] = &[
    "window-stack",
    "slash-circle",
    "arrow-left-right",
    "lightning-charge",
    "translate",
    "question-circle",
    "box-arrow-up-right",
    "link-45deg",
];

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
    // Prefixed by destination, not by icon name: the same icon may be wanted
    // in both places one day, and the two differ in size and colour.
    bake(
        &out_dir,
        "menu",
        MENU_ICONS,
        MENU_ICON_SIZE,
        MENU_ICON_COLOR,
    );
    bake(&out_dir, "ui", UI_ICONS, UI_ICON_SIZE, UI_ICON_COLOR);
}

fn bake(out_dir: &Path, prefix: &str, names: &[&str], size: u32, color: &str) {
    for name in names {
        let source = Path::new("assets/icons").join(format!("{name}.svg"));
        println!("cargo:rerun-if-changed={}", source.display());
        let rgba = rasterize(&source, size, color);
        std::fs::write(out_dir.join(format!("{prefix}-{name}.rgba")), rgba)
            .unwrap_or_else(|e| panic!("failed to write the rasterized {name}: {e}"));
    }
}

/// One SVG to straight (non-premultiplied) RGBA, which is what muda wants. A
/// failure is a build failure on purpose: a menu quietly losing its icons is
/// the kind of thing nobody notices for a release or two.
fn rasterize(source: &Path, size: u32, color: &str) -> Vec<u8> {
    let svg = std::fs::read_to_string(source)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", source.display()));
    let svg = svg.replace("currentColor", color);

    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", source.display()));
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(size, size).expect("an icon-sized pixmap is allocatable");
    let scale = size as f32 / tree.size().width();
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
