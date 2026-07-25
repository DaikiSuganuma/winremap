//! Bootstrap Icons for the settings window (ADR 0040).
//!
//! `build.rs` rasterizes the SVGs to RGBA, so egui never sees vector data —
//! it cannot draw SVG without pulling a rasterizer into the binary, which is
//! the whole point of doing it at build time.
//!
//! The faces are baked white and tinted at draw time, which is what lets one
//! set of pixels work in both the light and the dark theme.

use eframe::egui;

use crate::theme;

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
    Reload,
    Clear,
    Copy,
    /// The config folder in the address bar (v0.4 screen design §2).
    Folder,
    /// The general-settings entry in the navigation tree.
    Gear,
    /// The keymap group heading in the navigation tree.
    Keyboard,
    /// Enters edit mode.
    Pencil,
    /// Saves the draft.
    Floppy,
    /// Discards the draft (arrow-counterclockwise).
    Revert,
    /// Adds a row or a keymap.
    Plus,
    /// Deletes the selected keymap.
    Dash,
    /// Reorders the selected keymap.
    ArrowUp,
    ArrowDown,
    /// Deletes one table row.
    Close,
    /// The foreground-capture countdown (B4).
    Hourglass,
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
            Icon::Reload => "arrow-clockwise",
            Icon::Clear => "trash",
            Icon::Copy => "clipboard",
            Icon::Folder => "folder",
            Icon::Gear => "gear",
            Icon::Keyboard => "keyboard",
            Icon::Pencil => "pencil",
            Icon::Floppy => "floppy",
            Icon::Revert => "arrow-counterclockwise",
            Icon::Plus => "plus",
            Icon::Dash => "dash",
            Icon::ArrowUp => "arrow-up",
            Icon::ArrowDown => "arrow-down",
            Icon::Close => "x",
            Icon::Hourglass => "hourglass",
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
            Icon::Reload => include_bytes!(concat!(env!("OUT_DIR"), "/ui-arrow-clockwise.rgba")),
            Icon::Clear => include_bytes!(concat!(env!("OUT_DIR"), "/ui-trash.rgba")),
            Icon::Copy => include_bytes!(concat!(env!("OUT_DIR"), "/ui-clipboard.rgba")),
            Icon::Folder => include_bytes!(concat!(env!("OUT_DIR"), "/ui-folder.rgba")),
            Icon::Gear => include_bytes!(concat!(env!("OUT_DIR"), "/ui-gear.rgba")),
            Icon::Keyboard => include_bytes!(concat!(env!("OUT_DIR"), "/ui-keyboard.rgba")),
            Icon::Pencil => include_bytes!(concat!(env!("OUT_DIR"), "/ui-pencil.rgba")),
            Icon::Floppy => include_bytes!(concat!(env!("OUT_DIR"), "/ui-floppy.rgba")),
            Icon::Revert => {
                include_bytes!(concat!(env!("OUT_DIR"), "/ui-arrow-counterclockwise.rgba"))
            }
            Icon::Plus => include_bytes!(concat!(env!("OUT_DIR"), "/ui-plus.rgba")),
            Icon::Dash => include_bytes!(concat!(env!("OUT_DIR"), "/ui-dash.rgba")),
            Icon::ArrowUp => include_bytes!(concat!(env!("OUT_DIR"), "/ui-arrow-up.rgba")),
            Icon::ArrowDown => include_bytes!(concat!(env!("OUT_DIR"), "/ui-arrow-down.rgba")),
            Icon::Close => include_bytes!(concat!(env!("OUT_DIR"), "/ui-x.rgba")),
            Icon::Hourglass => include_bytes!(concat!(env!("OUT_DIR"), "/ui-hourglass.rgba")),
        }
    }
}

/// A button that leads with its icon, sized to the height of the button's own
/// label. Without an explicit size egui draws the texture at its source
/// resolution, which towers over the text (owner decision 2026-07-21).
pub fn button(ui: &mut egui::Ui, icon: Icon, text: &str) -> egui::Response {
    // The button's own text colour, not the panel's: on a button face they are
    // not always the same shade.
    let tint = ui.visuals().widgets.inactive.fg_stroke.color;
    let size = theme::button_icon_size(ui);
    let image = image(ui.ctx(), icon)
        .fit_to_exact_size(egui::vec2(size, size))
        .tint(tint);
    ui.add(egui::Button::image_and_text(image, text))
}

/// An icon-only button, for chrome where a label would crowd the row; the
/// caller supplies the name as a tooltip instead.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon) -> egui::Response {
    let tint = ui.visuals().widgets.inactive.fg_stroke.color;
    let size = theme::button_icon_size(ui);
    let image = image(ui.ctx(), icon)
        .fit_to_exact_size(egui::vec2(size, size))
        .tint(tint);
    ui.add(egui::Button::image(image))
}

/// A link that says where it goes: the icon marks it as leaving WinRemap.
///
/// Returns whether it was clicked. The icon is not part of the hit area —
/// egui has no widget for an image-plus-link, and the text is the target
/// people aim at anyway.
pub fn link(ui: &mut egui::Ui, icon: Icon, text: &str) -> bool {
    ui.horizontal(|ui| {
        let size = theme::body_icon_size(ui);
        show(ui, icon, size);
        ui.link(text).clicked()
    })
    .inner
}

/// Draws an icon `size` points square, in the current text colour.
pub fn show(ui: &mut egui::Ui, icon: Icon, size: f32) {
    let tint = ui.visuals().text_color();
    ui.add(
        image(ui.ctx(), icon)
            .fit_to_exact_size(egui::vec2(size, size))
            .tint(tint),
    );
}

/// The icon as an un-tinted, un-sized image, for callers that want to place it
/// themselves.
fn image(ctx: &egui::Context, icon: Icon) -> egui::Image<'static> {
    egui::Image::new(egui::load::SizedTexture::from_handle(&texture(ctx, icon)))
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
