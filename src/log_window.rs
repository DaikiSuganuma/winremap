//! The tray's "Show log" window: a live view of debug output for users who
//! did not start WinRemap from a terminal (ADR 0029).
//!
//! Three properties matter more than the UI itself:
//!
//! * The hook never touches this. Lines arrive from `drain_debug_log` on the
//!   message loop, which is already outside the callback (ADR 0016), so no
//!   locking is added to the latency-critical path (AGENTS.md invariant 2).
//! * The window runs on its own thread with its own event loop, so opening or
//!   closing it cannot stall the message loop that services the hook. winit
//!   allows only one event loop per process, so closing the window hides it
//!   and keeps the loop alive for the next "Show log" (ADR 0032).
//! * Nothing is written to disk. Debug output is key-name level and stays in
//!   memory only (AGENTS.md invariant 6).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use eframe::egui;

use crate::hook;
use crate::i18n;

/// Bounded so a long debug session cannot grow the buffer without limit.
const MAX_LINES: usize = 5000;

/// Whether the window is showing. Tells `emit` whether anyone is listening
/// and keeps a second click from re-seeding the buffer.
static OPEN: AtomicBool = AtomicBool::new(false);

/// Whether the event-loop thread exists. Once started it runs for the rest of
/// the process: winit refuses to build a second `EventLoop`, so a closed
/// window is a hidden one, not a destroyed one.
static LOOP_RUNNING: AtomicBool = AtomicBool::new(false);

/// Set by `open` and consumed by the next frame, which is the only place
/// allowed to talk to the viewport.
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The running window's context, used to wake its event loop from the tray
/// thread. eframe paints hidden windows directly for exactly this reason, so
/// a `Visible(true)` sent from the woken frame is honored.
fn wake_context() -> &'static Mutex<Option<egui::Context>> {
    static CTX: OnceLock<Mutex<Option<egui::Context>>> = OnceLock::new();
    CTX.get_or_init(|| Mutex::new(None))
}

/// Whether `--debug` was on the command line. Closing the window restores
/// this rather than assuming debug should go off.
static CLI_DEBUG: AtomicBool = AtomicBool::new(false);

fn buffer() -> &'static Mutex<VecDeque<String>> {
    static BUFFER: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    BUFFER.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Records the startup `--debug` value so the window can restore it on close.
pub fn set_cli_debug(enabled: bool) {
    CLI_DEBUG.store(enabled, Ordering::Relaxed);
}

/// One line of user-visible log output. Goes to the terminal when there is
/// one and to the window when it is open; with neither, it evaporates, which
/// is the intended silent-launch behavior.
///
/// Called from the message loop, never from the hook callback.
pub fn emit(line: &str) {
    crate::notify::console_line(line);
    if !OPEN.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut lines) = buffer().lock() {
        if lines.len() >= MAX_LINES {
            lines.pop_front();
        }
        lines.push_back(line.to_owned());
    }
}

/// Opens the log window, turning debug logging on for as long as it is up.
/// Clicking again while it is up just brings it to the front; the buffer is
/// only reset when the window was closed.
pub fn open() {
    let already_open = OPEN.swap(true, Ordering::SeqCst);
    if !already_open {
        if let Ok(mut lines) = buffer().lock() {
            lines.clear();
            lines.push_back(i18n::t().log_window_hint.to_owned());
        }
        hook::set_debug(true);
    }

    // The window exists but is hidden (or buried): ask the next frame to show
    // it, then wake the event loop so that frame happens now.
    if LOOP_RUNNING.load(Ordering::SeqCst) {
        SHOW_REQUESTED.store(true, Ordering::SeqCst);
        if let Ok(ctx) = wake_context().lock()
            && let Some(ctx) = ctx.as_ref()
        {
            ctx.request_repaint();
        }
        return;
    }

    // Its own thread, so the message loop keeps servicing the hook while the
    // window is up. If the thread cannot start, fall back to the previous
    // state instead of leaving debug logging stuck on.
    LOOP_RUNNING.store(true, Ordering::SeqCst);
    let spawned = std::thread::Builder::new()
        .name("winremap-log".to_owned())
        .spawn(run_window);
    if spawned.is_err() {
        LOOP_RUNNING.store(false, Ordering::SeqCst);
        OPEN.store(false, Ordering::SeqCst);
        hook::set_debug(CLI_DEBUG.load(Ordering::Relaxed));
    }
}

/// Leaves the window hidden but the event loop alive: debug logging goes back
/// to whatever the command line asked for and the buffer is released.
fn on_hidden() {
    OPEN.store(false, Ordering::SeqCst);
    hook::set_debug(CLI_DEBUG.load(Ordering::Relaxed));
    if let Ok(mut lines) = buffer().lock() {
        lines.clear();
    }
}

fn run_window() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(i18n::t().log_window_title)
            .with_inner_size([760.0, 480.0]),
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
        "winremap-log",
        options,
        Box::new(|cc| {
            install_fonts(&cc.egui_ctx);
            if let Ok(mut ctx) = wake_context().lock() {
                *ctx = Some(cc.egui_ctx.clone());
            }
            Ok(Box::new(LogApp::default()))
        }),
    );
    if let Err(e) = result {
        crate::notify::error(&i18n::log_window_failed(&e.to_string()));
    }

    // Only reached if the loop failed to start or died. Nothing may set
    // `LOOP_RUNNING` back to false while a loop is alive — a second
    // `EventLoop` in this process is exactly the error this avoids.
    if let Ok(mut ctx) = wake_context().lock() {
        *ctx = None;
    }
    LOOP_RUNNING.store(false, Ordering::SeqCst);
    on_hidden();
}

/// egui ships no CJK glyphs, so Japanese log lines would render as boxes.
/// Borrow a face from the system rather than embedding megabytes of font in
/// the exe; if none of the candidates exist the window still works, just
/// without Japanese glyphs.
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

struct LogApp {
    /// Stick to the newest line unless the user scrolls up to read history.
    follow_tail: bool,
}

impl Default for LogApp {
    fn default() -> Self {
        Self { follow_tail: true }
    }
}

impl eframe::App for LogApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if SHOW_REQUESTED.swap(false, Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // Closing hides the window instead of ending the event loop, which
        // could never be rebuilt (ADR 0032). From the user's side it is a
        // close: logging stops and the buffer is dropped.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            on_hidden();
        }

        // Lines are produced by another thread, so poll instead of waiting
        // for input events; idle cost is one cheap frame every 200 ms. While
        // hidden nothing is scheduled, so the thread sleeps until `open`
        // wakes it.
        if OPEN.load(Ordering::Relaxed) {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        let texts = i18n::t();
        egui::Panel::top("controls").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.follow_tail, texts.log_window_follow);
                if ui.button(texts.log_window_clear).clicked()
                    && let Ok(mut lines) = buffer().lock()
                {
                    lines.clear();
                }
                if ui.button(texts.log_window_copy).clicked()
                    && let Ok(lines) = buffer().lock()
                {
                    let joined = lines.iter().cloned().collect::<Vec<_>>().join("\r\n");
                    ui.ctx().copy_text(joined);
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .stick_to_bottom(self.follow_tail)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let Ok(lines) = buffer().lock() else { return };
                    for line in lines.iter() {
                        ui.label(egui::RichText::new(line).monospace());
                    }
                });
        });
    }
}
