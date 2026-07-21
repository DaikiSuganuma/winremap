//! One place for every size, spacing and colour the application draws with.
//!
//! Not a user-facing setting — this is a developer file (owner decision
//! 2026-07-21). The point is that "how big does the settings window open"
//! and "what colour is a table header" are answerable, and changeable, by
//! reading one file instead of hunting through the widget code.
//!
//! It covers both rendering stacks WinRemap uses: the egui windows
//! (settings, log) and the GDI overlay panels (the macro-recording banner,
//! the IME indicator). They cannot share types — egui has `Color32`, Win32
//! has `COLORREF` — but they can share a file, which is what makes "what
//! colour is that panel" a question with one place to look.
//!
//! **egui sizes are constants; egui colours are functions.** Every colour here is
//! derived from egui's current `Visuals` rather than written as an RGB
//! value, because both windows follow the system light/dark theme. A
//! hardcoded colour would look right in one theme and wrong in the other.
//! What this module fixes is the *relationship* — the header band takes the
//! text colour and the header text takes the background, and that is what
//! "reversed" means here.

use eframe::egui;
use windows::Win32::Foundation::COLORREF;

// ---- Window sizes --------------------------------------------------------

/// The settings window, in points. Tall enough that a keymap's apps, rules
/// and the notes under them fit without scrolling on a 1080p screen (owner
/// decision 2026-07-21). Not remembered between openings, so this is what
/// people see every time.
pub const CONFIG_WINDOW: [f32; 2] = [1120.0, 860.0];

/// The log window, in points. Wide enough for a debug line without wrapping.
pub const LOG_WINDOW: [f32; 2] = [760.0, 480.0];

/// The invisible host viewport that owns the event loop (ADR 0037). One
/// pixel, parked off-screen: eframe shows its root window after the first
/// frame no matter what, so it is built to be harmless when shown.
pub const HOST_WINDOW: [f32; 2] = [1.0, 1.0];
pub const HOST_POSITION: [f32; 2] = [-32000.0, -32000.0];

// ---- Type scale ----------------------------------------------------------

/// Section titles, bigger than body text and sitting under a rule, so a long
/// detail pane reads as parts rather than one wall.
pub const SECTION_TEXT: f32 = 17.0;
/// The keymap's own name, one step above its sections.
pub const TITLE_TEXT: f32 = 21.0;
/// The recorded macro, which is the one value in the settings window that
/// changes while you watch it.
pub const HIGHLIGHT_TEXT: f32 = 16.0;

// ---- Spacing -------------------------------------------------------------

/// Room around the text inside every button and checkbox (owner decision
/// 2026-07-21). Buttons are wider than they are tall: a label reads as
/// cramped long before it looks short.
pub const BUTTON_PADDING: f32 = 8.0;
pub const BUTTON_SIDE_PADDING: f32 = 8.0;

/// Room around cell text. Applied as grid spacing, so half of it lands on
/// each side of the gap between two cells.
pub const CELL_PAD: i8 = 4;

/// Room between a table's own border and the cells at its edges. Wider than
/// the gap between cells, because text touching a rule is hard to read; kept
/// equal on both sides so the header band stays centred in the frame.
pub const EDGE_PAD: i8 = 8;

/// A note reads as belonging to the table it sits under only if there is a
/// clear break between them.
pub const NOTE_GAP: f32 = 8.0;

/// Whitespace between sections of the detail pane. There is no rule between
/// them (owner decision 2026-07-21: the window had accumulated too many
/// lines to read comfortably), so this gap is the only thing separating one
/// section from the last — which is why it is generous.
pub const SECTION_GAP: f32 = 20.0;

/// Margin inside a window's chrome — the log window's header and footer,
/// the settings window's file header. Equal on all four sides so the
/// controls sit clear of both the window edge and the content they top or
/// tail (owner decision 2026-07-21).
pub const PANEL_PAD: i8 = 8;

/// The config-file table's share of the header's width. Half, so the
/// controls that act on the file have the other half; stretching the table
/// across the whole window pushed each value a long way from its label
/// (owner decision 2026-07-21).
pub const FILE_TABLE_WIDTH_RATIO: f32 = 0.5;

/// Padding inside a highlighted box, so its fill reads as a surface rather
/// than as ink spilled behind the text.
pub const HIGHLIGHT_PAD: i8 = 10;

/// Corner rounding for highlighted boxes.
pub const HIGHLIGHT_ROUNDING: u8 = 6;

// ---- Icons ---------------------------------------------------------------

/// Icons are sized to the text they sit beside rather than to a constant, so
/// they match at any font scale (owner decision 2026-07-21: they were
/// drawing at their source resolution and towering over the labels).
pub fn icon_size(ui: &egui::Ui, style: &egui::TextStyle) -> f32 {
    ui.text_style_height(style)
}

/// An icon on a button matches the button label's height.
pub fn button_icon_size(ui: &egui::Ui) -> f32 {
    icon_size(ui, &egui::TextStyle::Button)
}

/// An icon beside body text — a link, a section heading — matches that text.
pub fn body_icon_size(ui: &egui::Ui) -> f32 {
    icon_size(ui, &egui::TextStyle::Body)
}

// ---- Colours -------------------------------------------------------------

/// How far a window's chrome is pushed from the panel background toward the
/// text colour. Just enough to read as a band rather than as part of the
/// content — the windows had too many hairlines already, so the separation
/// is carried by tone instead of by a rule.
const CHROME_FILL_LERP: f32 = 0.05;

/// How far a highlighted box's fill is pushed from the panel background
/// toward the text colour. Enough to read as a distinct surface, not so far
/// that it competes with the tables around it.
const HIGHLIGHT_FILL_LERP: f32 = 0.10;

/// Fill for a header or footer band.
pub fn chrome_fill(visuals: &egui::Visuals) -> egui::Color32 {
    visuals
        .panel_fill
        .lerp_to_gamma(visuals.text_color(), CHROME_FILL_LERP)
}

/// The frame a header or footer panel draws itself with: the band's fill
/// plus the margin its controls sit in.
pub fn chrome_frame(visuals: &egui::Visuals) -> egui::Frame {
    egui::Frame::new()
        .fill(chrome_fill(visuals))
        .inner_margin(egui::Margin::same(PANEL_PAD))
}

/// The stroke enclosing a table.
pub fn table_border(visuals: &egui::Visuals) -> egui::Stroke {
    visuals.widgets.noninteractive.bg_stroke
}

/// The header band's fill: the text colour, so the row reads as reversed.
pub fn table_header_bg(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.text_color()
}

/// The header band's text: the window background, the other half of the
/// reversal. Following the theme rather than being fixed is what keeps it
/// readable in both light and dark.
pub fn table_header_text(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.extreme_bg_color
}

/// Zebra striping for odd body rows.
pub fn table_stripe(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.faint_bg_color
}

/// Fill for a box that has to stand out from the tables around it.
pub fn highlight_fill(visuals: &egui::Visuals) -> egui::Color32 {
    visuals
        .panel_fill
        .lerp_to_gamma(visuals.text_color(), HIGHLIGHT_FILL_LERP)
}

/// The stroke around a highlighted box.
pub fn highlight_stroke(visuals: &egui::Visuals) -> egui::Stroke {
    visuals.widgets.noninteractive.bg_stroke
}

// ---- Overlay panels (Win32/GDI) -----------------------------------------

// The macro-recording banner (`macro_record/banner.rs`) and the IME
// indicator (`ime_indicator/overlay.rs`) are layered GDI windows, not egui.
// They do not follow the system light/dark theme — a translucent panel
// floating over someone else's window has no background to match — so unlike
// the values above these are fixed.

/// Near-black panel body, shared by both overlays so they read as parts of
/// the same application.
pub const OVERLAY_BG: COLORREF = COLORREF(0x0020_1C1C);
/// Primary text on that body.
pub const OVERLAY_TEXT: COLORREF = COLORREF(0x00FF_FFFF);
/// Secondary text — the app name under the IME glyph.
pub const OVERLAY_LABEL: COLORREF = COLORREF(0x00D0_D0D0);

/// Face used by both overlays. GDI font substitution covers its absence.
pub const OVERLAY_FONT_FACE: &str = "Yu Gothic UI";
/// Semibold: reads better than regular at high translucency.
pub const OVERLAY_FONT_WEIGHT: i32 = 600;

/// Banner alpha. Fixed, unlike the IME panel's configurable opacity: this is
/// a status line rather than a decoration.
pub const BANNER_OPACITY: u8 = 230;
/// LOGFONT height; negative means character height rather than cell height.
pub const BANNER_FONT_HEIGHT: i32 = -18;
/// Room left and right of the banner's line.
pub const BANNER_PADDING_X: i32 = 20;
pub const BANNER_HEIGHT: i32 = 44;
/// Gap between the banner and the bottom of the work area, so it clears the
/// taskbar without sitting flush against it.
pub const BANNER_MARGIN_BOTTOM: i32 = 24;
/// Never take more than this share of the work area's width; longer lines
/// get an ellipsis instead.
pub const BANNER_MAX_WIDTH_PERCENT: i32 = 80;
