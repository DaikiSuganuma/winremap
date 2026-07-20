//! The GUI thread: one winit event loop hosting every WinRemap window.
//!
//! winit allows a single `EventLoop` per process, so all windows share one
//! (ADR 0035). The root viewport is the config window; the log window is a
//! deferred child viewport created and destroyed on demand.
//!
//! Properties that matter more than the UI itself:
//!
//! * The hook never touches this. Log lines arrive from `drain_debug_log` on
//!   the message loop, which is already outside the callback (ADR 0016), so no
//!   locking is added to the latency-critical path (AGENTS.md invariant 2).
//! * The loop runs on its own thread, so opening or closing a window cannot
//!   stall the message loop that services the hook. Closing the root window
//!   hides it rather than ending the loop, which could never be rebuilt
//!   (ADR 0032).
//! * With every window closed nothing is scheduled, so the thread sleeps.
//! * Only `config.toml` is ever written. Logs stay in memory (invariant 6).
//! * No `unsafe` here: the GUI reaches Win32 through the existing modules
//!   (invariant 3).

pub mod config_window;
pub mod log;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use eframe::egui;

use crate::i18n;

/// Repaint cadence while a window is up. Log lines are produced by another
/// thread, so the GUI polls instead of waiting for input events.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Whether the event-loop thread exists. Once started it runs for the rest of
/// the process — see the module docs.
static LOOP_RUNNING: AtomicBool = AtomicBool::new(false);

/// The config window (root viewport) is showing.
static CONFIG_OPEN: AtomicBool = AtomicBool::new(false);

/// Set by `open_config`, consumed by the next frame — the only place allowed
/// to talk to the viewport.
static SHOW_CONFIG: AtomicBool = AtomicBool::new(false);

/// The running loop's context, used to wake it from the tray thread. eframe
/// paints hidden windows directly (egui Issue #5229), so a `Visible(true)`
/// sent from the woken frame is honored.
fn wake_context() -> &'static Mutex<Option<egui::Context>> {
    static CTX: OnceLock<Mutex<Option<egui::Context>>> = OnceLock::new();
    CTX.get_or_init(|| Mutex::new(None))
}

fn config_path() -> &'static Mutex<PathBuf> {
    static PATH: OnceLock<Mutex<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| Mutex::new(PathBuf::new()))
}

/// Records which config file this run uses, so the GUI can show and open it.
pub fn set_config_path(path: PathBuf) {
    if let Ok(mut slot) = config_path().lock() {
        *slot = path;
    }
}

/// Opens the config window, or brings it to the front if it is already up.
pub fn open_config() {
    CONFIG_OPEN.store(true, Ordering::SeqCst);
    SHOW_CONFIG.store(true, Ordering::SeqCst);
    ensure_loop();
}

/// Opens the log window. Debug logging stays on for as long as it is up.
pub fn open_log() {
    log::request_open();
    ensure_loop();
}

/// Starts the GUI thread on first use, or wakes the running one so the
/// pending show request is handled now rather than at the next poll.
fn ensure_loop() {
    if LOOP_RUNNING.load(Ordering::SeqCst) {
        if let Ok(ctx) = wake_context().lock()
            && let Some(ctx) = ctx.as_ref()
        {
            ctx.request_repaint();
        }
        return;
    }

    // Its own thread, so the message loop keeps servicing the hook while a
    // window is up. If the thread cannot start, undo the pending requests
    // rather than leaving debug logging stuck on.
    LOOP_RUNNING.store(true, Ordering::SeqCst);
    let spawned = std::thread::Builder::new()
        .name("winremap-gui".to_owned())
        .spawn(run_loop);
    if spawned.is_err() {
        LOOP_RUNNING.store(false, Ordering::SeqCst);
        CONFIG_OPEN.store(false, Ordering::SeqCst);
        SHOW_CONFIG.store(false, Ordering::SeqCst);
        log::on_closed();
    }
}

fn run_loop() {
    // Hidden at first: `open_config` / `open_log` decide what the user sees,
    // and a window must never appear on a silent launch (ADR 0029).
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(i18n::t().config_window_title)
        .with_inner_size([960.0, 640.0])
        .with_visible(false);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        // winit refuses a non-main-thread event loop unless asked; this is
        // what keeps the hook's message loop on the main thread untouched.
        // `winit` is a direct dependency only for this trait; it must stay
        // pinned to the version eframe uses, or the extension no longer
        // applies to eframe's builder (a compile error, not a silent bug).
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };

    let result = eframe::run_native(
        "winremap-gui",
        options,
        Box::new(|cc| {
            install_fonts(&cc.egui_ctx);
            // Underlined IME preedit instead of egui's Windows default, which
            // draws it as a plain selection (ADR 0034).
            cc.egui_ctx.all_styles_mut(|style| {
                style.visuals.ime_composition.legacy_visuals = false;
            });
            if let Ok(mut ctx) = wake_context().lock() {
                *ctx = Some(cc.egui_ctx.clone());
            }
            Ok(Box::<GuiApp>::default())
        }),
    );
    if let Err(e) = result {
        crate::notify::error(&i18n::gui_failed(&e.to_string()));
    }

    // Only reached if the loop failed to start or died. Nothing may set
    // `LOOP_RUNNING` back to false while a loop is alive — a second
    // `EventLoop` in this process is exactly the error this avoids.
    if let Ok(mut ctx) = wake_context().lock() {
        *ctx = None;
    }
    LOOP_RUNNING.store(false, Ordering::SeqCst);
    CONFIG_OPEN.store(false, Ordering::SeqCst);
    log::on_closed();
}

/// The title-bar / taskbar / Alt+Tab icon: the same keyboard mark as the tray
/// and the exe. winit takes raw pixels rather than an .ico, so the 48 px PNG
/// is decoded at startup — Windows only ever scales it down from there, and
/// eframe already depends on a PNG decoder, so this costs no new crate.
/// `None` if it ever fails to decode; the window just gets the default icon.
fn window_icon() -> Option<std::sync::Arc<egui::IconData>> {
    let png = include_bytes!("../../assets/png/kbd-enabled-48.png");
    eframe::icon_data::from_png_bytes(png)
        .ok()
        .map(std::sync::Arc::new)
}

/// egui ships no CJK glyphs, so Japanese text would render as boxes. Borrow a
/// face from the system rather than embedding megabytes of font in the exe; if
/// none of the candidates exist the GUI still works, just without Japanese
/// glyphs.
fn install_fonts(ctx: &egui::Context) {
    // Ordered by how likely they are to be present and readable at small
    // sizes. `.ttc` files are collections, hence the face index.
    const CANDIDATES: &[(&str, u32)] = &[
        (r"C:\Windows\Fonts\meiryo.ttc", 0),
        (r"C:\Windows\Fonts\YuGothM.ttc", 0),
        (r"C:\Windows\Fonts\YuGothR.ttc", 0),
        (r"C:\Windows\Fonts\msgothic.ttc", 0),
    ];
    let Some((bytes, index)) = CANDIDATES
        .iter()
        .find_map(|&(path, index)| std::fs::read(path).ok().map(|bytes| (bytes, index)))
    else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    let mut data = egui::FontData::from_owned(bytes);
    data.index = index;
    fonts.font_data.insert("system_jp".to_owned(), data.into());
    // Appended, not prepended: egui's own font keeps its tuned Latin shapes
    // and the system face fills in the glyphs it lacks.
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("system_jp".to_owned());
    }
    ctx.set_fonts(fonts);
}

#[derive(Default)]
struct GuiApp {
    config: config_window::ConfigWindow,
}

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if SHOW_CONFIG.swap(false, Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // Closing hides the root window instead of ending the event loop
        // (ADR 0032). From the user's side it is a close.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            CONFIG_OPEN.store(false, Ordering::SeqCst);
        }

        log::show_viewport(&ctx);

        // A deferred viewport only survives while its parent keeps declaring
        // it, so the root has to keep painting while the log window is up —
        // even hidden, which eframe supports (ADR 0035). With both windows
        // closed nothing is scheduled and the thread sleeps.
        if CONFIG_OPEN.load(Ordering::Relaxed) || log::is_open() {
            ctx.request_repaint_after(POLL_INTERVAL);
        }

        self.config.ui(ui);
    }
}
