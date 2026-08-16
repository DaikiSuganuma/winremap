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
//! * The only things ever written are the config file and the one line
//!   recording which config file that is (ADR 0077). Logs stay in memory
//!   (invariant 6).
//! * No `unsafe` in this file: what egui cannot express — per-size window
//!   icons, handing the config file to the shell — lives in `win32`
//!   (invariant 3, ADR 0038).

pub mod config_window;
mod icons;
pub mod log;
mod watch;
mod win32;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use eframe::egui;

use crate::i18n;
use crate::theme;

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

/// The settings window asked for a reload. It cannot do one itself: the tray
/// icon and its tooltip belong to the thread that created them, so the request
/// is picked up by the message loop instead.
static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The `(mtime, size)` the active config file had when it was last loaded.
/// The address bar's change mark (●) is "the file no longer matches this"
/// (v0.4 screen design §2.2).
fn loaded_stamp_slot() -> &'static Mutex<Option<winremap::config::draft::FileStamp>> {
    static STAMP: OnceLock<Mutex<Option<winremap::config::draft::FileStamp>>> = OnceLock::new();
    STAMP.get_or_init(|| Mutex::new(None))
}

/// Records that the config was just loaded. Called for the startup load and
/// for every successful reload, whoever asked for it. Stamps the file and
/// reports to the status bar.
pub fn mark_config_loaded() {
    let stamp = winremap::config::draft::stamp(&active_config_path()).ok();
    if let Ok(mut slot) = loaded_stamp_slot().lock() {
        *slot = stamp;
    }
    set_status(i18n::t().status_loaded);
}

pub fn loaded_stamp() -> Option<winremap::config::draft::FileStamp> {
    loaded_stamp_slot().lock().ok().and_then(|stamp| *stamp)
}

/// When this process started, formatted once for the status bar.
fn started_at_slot() -> &'static OnceLock<String> {
    static AT: OnceLock<String> = OnceLock::new();
    &AT
}

/// Called once from startup; the status bar shows this for the process's
/// whole life.
pub fn mark_started() {
    let _ = started_at_slot().set(crate::clock::local_now());
}

pub fn started_at() -> &'static str {
    started_at_slot().get().map(String::as_str).unwrap_or("")
}

/// The status bar's message slot: the latest thing that happened, replaced
/// by the next one, never cleared on its own (v0.4 screen design §5).
fn status_slot() -> &'static Mutex<String> {
    static STATUS: OnceLock<Mutex<String>> = OnceLock::new();
    STATUS.get_or_init(|| Mutex::new(String::new()))
}

pub fn set_status(text: &str) {
    if let Ok(mut slot) = status_slot().lock() {
        *slot = text.to_owned();
    }
}

pub fn status() -> String {
    status_slot()
        .lock()
        .map(|slot| slot.clone())
        .unwrap_or_default()
}

/// Asks the message loop for a reload; see `RELOAD_REQUESTED`. The wake is
/// what makes the button feel immediate — that loop is blocked in
/// `GetMessageW` whenever the user is not typing.
pub fn request_reload() {
    RELOAD_REQUESTED.store(true, Ordering::SeqCst);
    crate::hook::wake_message_loop();
}

/// Consumed by the message loop once per pass.
pub fn take_reload_request() -> bool {
    RELOAD_REQUESTED.swap(false, Ordering::SeqCst)
}

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

/// Records which config file this run uses. Set at startup and by the
/// address bar's file switch (ADR 0050).
pub fn set_config_path(path: PathBuf) {
    if let Ok(mut slot) = config_path().lock() {
        *slot = path;
    }
}

/// The active config file — the single source of truth (ADR 0050). Every
/// reader goes through this: the settings window, the editor launcher, and
/// the tray's reload, so switching the file switches them all.
pub fn active_config_path() -> PathBuf {
    config_path()
        .lock()
        .map(|path| path.clone())
        .unwrap_or_default()
}

/// Where the choice of config file is remembered between runs (ADR 0077).
/// `None` when there is nowhere to keep it, which makes a switch last for
/// this run only — the behaviour of every version before 1.0.0.
fn config_memory() -> &'static OnceLock<Option<PathBuf>> {
    static MEMORY: OnceLock<Option<PathBuf>> = OnceLock::new();
    &MEMORY
}

/// Called once from startup, before any window can be opened.
pub fn set_config_memory(path: Option<PathBuf>) {
    let _ = config_memory().set(path);
}

/// The address bar's file switch: the file becomes the active one **and** the
/// one the next start opens (ADR 0077).
///
/// Startup does not come through here, on purpose. Choosing a file is
/// something the user does in this window; a `--config` on the command line
/// is an instruction for one run, and must not overwrite the choice — the
/// acceptance probe alone starts WinRemap on a throwaway config half a dozen
/// times per run.
pub(crate) fn choose_config_path(path: PathBuf) {
    if let Some(memory) = config_memory().get().and_then(Option::as_ref)
        && let Err(e) = winremap::config::last_used::remember(memory, &path)
    {
        // Said out loud rather than swallowed: what goes wrong here is
        // invisible until the *next* start opens the old file, which is the
        // confusion this feature exists to remove.
        log::emit(&i18n::remember_config_failed(&e.to_string()));
    }
    set_config_path(path);
}

/// Hides the settings window (close = hide, ADR 0032). For the footer's
/// "close without saving" — the ordinary close goes through the viewport.
pub(crate) fn hide_config() {
    CONFIG_OPEN.store(false, Ordering::SeqCst);
    log::action(&i18n::action_closed(i18n::t().config_window_title));
}

/// Opens the settings window, or brings it to the front if it is already up.
pub fn open_config() {
    log::action(i18n::t().menu_settings);
    CONFIG_OPEN.store(true, Ordering::SeqCst);
    FOCUS_CONFIG.store(true, Ordering::SeqCst);
    ensure_loop();
}

/// Opens the log window. Debug logging stays on for as long as it is up.
pub fn open_log() {
    // After request_open: opening a closed window clears the buffer, so the
    // action line has to land on the fresh one.
    log::request_open();
    log::action(i18n::t().menu_log);
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
        .with_inner_size(theme::HOST_WINDOW)
        .with_position(theme::HOST_POSITION)
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
                // egui's default is 6x2, which gives a button barely taller
                // than its text. These are clicked with a mouse, not aimed at
                // with a controller, but a comfortable target still helps.
                style.spacing.button_padding = egui::vec2(
                    theme::BUTTON_PADDING + theme::BUTTON_SIDE_PADDING,
                    theme::BUTTON_PADDING,
                );
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
struct HostApp {
    /// Behind a lock because a deferred viewport's callback must be `Fn` and
    /// outlive the frame that declared it.
    config: Arc<Mutex<config_window::ConfigWindow>>,
    /// Which windows were up last frame, so icons are re-applied when one
    /// appears rather than on every frame.
    windows_shown: (bool, bool),
    /// Frames left to re-apply icons and re-hide the host. Both have to
    /// outlast the frame that asked for the window, because the window itself
    /// only exists from the next one.
    settle_frames: u8,
}

impl Default for HostApp {
    fn default() -> Self {
        Self {
            config: Arc::default(),
            windows_shown: (false, false),
            // Non-zero so the host is hidden right after eframe reveals it.
            settle_frames: SETTLE_FRAMES,
        }
    }
}

/// How many frames a window change keeps `set_window_icons` running.
const SETTLE_FRAMES: u8 = 3;

impl eframe::App for HostApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // The folder watch follows the settings window (ADR 0051): here on
        // the host frame, because this runs whether or not that window is —
        // which is exactly when a close has to release the watch.
        let config_open = CONFIG_OPEN.load(Ordering::Relaxed);
        let path = active_config_path();
        watch::sync(config_open, path.parent(), &ctx);

        show_config_viewport(&ctx, &self.config);
        log::show_viewport(&ctx);

        let shown = (CONFIG_OPEN.load(Ordering::Relaxed), log::is_open());
        if shown != self.windows_shown {
            self.windows_shown = shown;
            self.settle_frames = SETTLE_FRAMES;
        }
        if self.settle_frames > 0 {
            self.settle_frames -= 1;
            // egui only ever sets ICON_SMALL, so the icons are put on the
            // windows directly (ADR 0038). Cheap, and re-setting is a no-op.
            win32::set_window_icons();
            // eframe reveals the host after its first frame (ADR 0037). It is
            // off-screen and has no taskbar button, but a visible window still
            // shows up in Task Manager's window list, so hide it again.
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            ctx.request_repaint();
        }

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
    let builder = egui::ViewportBuilder::default()
        .with_title(i18n::t().config_window_title)
        .with_inner_size(theme::CONFIG_WINDOW);
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
            // to the host, so nothing is lost (ADR 0037). An unsaved draft
            // intercepts the close and asks in the footer instead (v0.4
            // screen design §7.3).
            if ctx.input(|i| i.viewport().close_requested()) {
                let intercepted = state
                    .lock()
                    .map(|mut window| window.intercept_close())
                    .unwrap_or(false);
                if intercepted {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                } else {
                    CONFIG_OPEN.store(false, Ordering::SeqCst);
                    log::action(&i18n::action_closed(i18n::t().config_window_title));
                }
            }
            if let Ok(mut window) = state.lock() {
                window.ui(ui);
            }
        },
    );
}
