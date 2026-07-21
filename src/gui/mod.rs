//! The GUI thread: one winit event loop hosting every WinRemap window.
//!
//! winit allows a single `EventLoop` per process, so all windows share one
//! (ADR 0035). eframe's root viewport is not one of them: it is an invisible
//! 1x1 host, and both real windows — settings and log — are deferred child
//! viewports it declares (ADR 0037). eframe reveals the root itself after its
//! first frame, which no amount of `with_visible(false)` prevents, so the root
//! has to be a window that does not matter when shown.
//!
//! Properties that matter more than the UI itself:
//!
//! * The hook never touches this. Log lines arrive from `drain_debug_log` on
//!   the message loop, which is already outside the callback (ADR 0016), so no
//!   locking is added to the latency-critical path (AGENTS.md invariant 2).
//! * The loop runs on its own thread, so opening or closing a window cannot
//!   stall the message loop that services the hook. The loop itself is never
//!   torn down; it could never be rebuilt (ADR 0032).
//! * With every window closed nothing is scheduled, so the thread sleeps.
//! * Only `config.toml` is ever written. Logs stay in memory (invariant 6).
//! * No `unsafe` here: the GUI reaches Win32 through the existing modules
//!   (invariant 3).

pub mod config_window;
pub mod log;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use eframe::egui;

use crate::i18n;

/// Repaint cadence for the invisible host while a window is up. It only has to
/// keep the children declared, so it can be slower than they are.
const HOST_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Whether the event-loop thread exists. Once started it runs for the rest of
/// the process — see the module docs.
static LOOP_RUNNING: AtomicBool = AtomicBool::new(false);

/// The settings window is showing.
static CONFIG_OPEN: AtomicBool = AtomicBool::new(false);

/// Set by `open_config`, consumed by the settings window's own frame — the
/// only place allowed to talk to its viewport.
static FOCUS_CONFIG: AtomicBool = AtomicBool::new(false);

/// The running loop's context, used to wake it from the tray thread. eframe
/// paints hidden windows directly (egui Issue #5229), so the host wakes even
/// while it has nothing on screen.
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

/// Opens the settings window, or brings it to the front if it is already up.
pub fn open_config() {
    CONFIG_OPEN.store(true, Ordering::SeqCst);
    FOCUS_CONFIG.store(true, Ordering::SeqCst);
    ensure_loop();
}

/// Opens the log window. Debug logging stays on for as long as it is up.
pub fn open_log() {
    log::request_open();
    ensure_loop();
}

/// Starts the GUI thread on first use, or wakes the running one so the pending
/// open is handled now rather than at the next poll.
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
        FOCUS_CONFIG.store(false, Ordering::SeqCst);
        log::on_closed();
    }
}

fn run_loop() {
    // The host window. eframe shows it after its first frame no matter what
    // (egui PR #2279), so it is built to be harmless when shown: one pixel,
    // no decorations, no taskbar button, parked far off-screen (ADR 0037).
    let host = egui::ViewportBuilder::default()
        .with_title("winremap")
        .with_inner_size([1.0, 1.0])
        .with_position([-32000.0, -32000.0])
        .with_decorations(false)
        .with_taskbar(false)
        .with_visible(false);

    let options = eframe::NativeOptions {
        viewport: host,
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
            Ok(Box::<HostApp>::default())
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

/// The window icon: the same keyboard mark as the tray and the exe. winit
/// takes raw pixels rather than an .ico, so a PNG is decoded here — eframe
/// already depends on a PNG decoder, so this costs no new crate.
///
/// egui-winit installs it as `ICON_SMALL` only (winit's `set_window_icon`),
/// and leaves `ICON_BIG` unset, so Windows scales this one bitmap to every
/// size it needs. `None` if it ever fails to decode; the window then gets the
/// default icon.
fn window_icon() -> Option<Arc<egui::IconData>> {
    let png = include_bytes!("../../assets/png/kbd-enabled-32.png");
    eframe::icon_data::from_png_bytes(png).ok().map(Arc::new)
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

/// Owns nothing on screen: it declares the real windows and keeps them alive.
#[derive(Default)]
struct HostApp {
    /// Behind a lock because a deferred viewport's callback must be `Fn` and
    /// outlive the frame that declared it.
    config: Arc<Mutex<config_window::ConfigWindow>>,
}

impl eframe::App for HostApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        show_config_viewport(&ctx, &self.config);
        log::show_viewport(&ctx);

        // A deferred viewport only survives while its parent keeps declaring
        // it, so the host has to keep painting while any window is up — which
        // eframe supports for hidden windows (ADR 0035). With everything
        // closed nothing is scheduled and the thread sleeps.
        if CONFIG_OPEN.load(Ordering::Relaxed) || log::is_open() {
            ctx.request_repaint_after(HOST_POLL_INTERVAL);
        }
    }
}

/// Declares the settings window for this frame. Not calling it is what closes
/// the window.
fn show_config_viewport(ctx: &egui::Context, state: &Arc<Mutex<config_window::ConfigWindow>>) {
    if !CONFIG_OPEN.load(Ordering::Relaxed) {
        return;
    }
    let mut builder = egui::ViewportBuilder::default()
        .with_title(i18n::t().config_window_title)
        .with_inner_size([960.0, 640.0]);
    if let Some(icon) = window_icon() {
        builder = builder.with_icon(icon);
    }
    let state = state.clone();
    ctx.show_viewport_deferred(
        egui::ViewportId::from_hash_of("winremap-settings"),
        builder,
        move |ui, _class| {
            let ctx = ui.ctx().clone();
            if FOCUS_CONFIG.swap(false, Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            // Closing a child viewport may destroy it: the event loop belongs
            // to the host, so nothing is lost (ADR 0037).
            if ctx.input(|i| i.viewport().close_requested()) {
                CONFIG_OPEN.store(false, Ordering::SeqCst);
            }
            if let Ok(mut window) = state.lock() {
                window.ui(ui);
            }
        },
    );
}
