//! Bootstrap Icons for the settings window (ADR 0040).
//!
//! `build.rs` rasterizes the SVGs to RGBA, so egui never sees vector data —
//! it cannot draw SVG without pulling a rasterizer into the binary, which is
//! the whole point of doing it at build time.
//!
//! The faces are baked white and tinted at draw time, which is what lets one
//! set of pixels work in both the light and the dark theme.

use eframe::egui;

/// Matches `UI_ICON_SIZE` in build.rs. Rasterized at twice the size icons are
/// drawn at, so they stay sharp on a HiDPI display.
const SOURCE_SIZE: usize = 32;

/// Named for what they mark rather than for the Bootstrap icon behind them —
/// the drawing can be swapped without touching callers.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Apps,
    Exclude,
    Rules,
    Macro,
    Ime,
    Notation,
    /// Leaves WinRemap for another application.
    External,
    /// Leaves WinRemap for the browser.
    Link,
}

impl Icon {
    /// Doubles as the texture's debug name and its cache key.
    fn name(self) -> &'static str {
        match self {
            Icon::Apps => "window-stack",
            Icon::Exclude => "slash-circle",
            Icon::Rules => "arrow-left-right",
            Icon::Macro => "lightning-charge",
            Icon::Ime => "translate",
            Icon::Notation => "question-circle",
            Icon::External => "box-arrow-up-right",
            Icon::Link => "link-45deg",
        }
    }

    fn pixels(self) -> &'static [u8] {
        match self {
            Icon::Apps => include_bytes!(concat!(env!("OUT_DIR"), "/ui-window-stack.rgba")),
            Icon::Exclude => include_bytes!(concat!(env!("OUT_DIR"), "/ui-slash-circle.rgba")),
            Icon::Rules => include_bytes!(concat!(env!("OUT_DIR"), "/ui-arrow-left-right.rgba")),
            Icon::Macro => include_bytes!(concat!(env!("OUT_DIR"), "/ui-lightning-charge.rgba")),
            Icon::Ime => include_bytes!(concat!(env!("OUT_DIR"), "/ui-translate.rgba")),
            Icon::Notation => include_bytes!(concat!(env!("OUT_DIR"), "/ui-question-circle.rgba")),
            Icon::External => {
                include_bytes!(concat!(env!("OUT_DIR"), "/ui-box-arrow-up-right.rgba"))
            }
            Icon::Link => include_bytes!(concat!(env!("OUT_DIR"), "/ui-link-45deg.rgba")),
        }
    }
}

/// A link that says where it goes: the icon marks it as leaving WinRemap.
///
/// Returns whether it was clicked. The icon is not part of the hit area —
/// egui has no widget for an image-plus-link, and the text is the target
/// people aim at anyway.
pub fn link(ui: &mut egui::Ui, icon: Icon, text: &str) -> bool {
    ui.horizontal(|ui| {
        show(ui, icon, LINK_ICON_SIZE);
        ui.link(text).clicked()
    })
    .inner
}

/// Link icons sit next to body text, so they match its height.
const LINK_ICON_SIZE: f32 = 14.0;

/// Draws an icon `size` points square, in the current text colour.
pub fn show(ui: &mut egui::Ui, icon: Icon, size: f32) {
    let texture = texture(ui.ctx(), icon);
    let tint = ui.visuals().text_color();
    ui.add(
        egui::Image::new(egui::load::SizedTexture::from_handle(&texture))
            .fit_to_exact_size(egui::vec2(size, size))
            .tint(tint),
    );
}

/// The uploaded texture, uploaded once per context rather than per frame.
///
/// The lookup and the insert are deliberately separate calls: egui guards all
/// of `Context` with one lock, so loading a texture while holding `data_mut`
/// would deadlock.
fn texture(ctx: &egui::Context, icon: Icon) -> egui::TextureHandle {
    let id = egui::Id::new(("winremap-icon", icon.name()));
    if let Some(handle) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)) {
        return handle;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([SOURCE_SIZE; 2], icon.pixels());
    let handle = ctx.load_texture(icon.name(), image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(id, handle.clone()));
    handle
}
