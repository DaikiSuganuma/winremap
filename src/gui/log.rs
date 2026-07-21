//! The tray's "Show log" window: a live view of debug output for users who did
//! not start WinRemap from a terminal (ADR 0029).
//!
//! It is a deferred child viewport of the config window (ADR 0035), so closing
//! it destroys the window while the shared event loop keeps running. Opening
//! it again simply declares it again on the next frame.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use eframe::egui;

use super::icons::{self, Icon};
use crate::hook;
use crate::i18n;
use crate::theme;

/// Bounded so a long debug session cannot grow the buffer without limit.
const MAX_LINES: usize = 5000;

/// How often the window redraws while it is up. Lines are produced by another
/// thread, so it polls rather than waiting for input events.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Whether the window is showing. Tells `emit` whether anyone is listening and
/// keeps a second click from re-seeding the buffer.
static OPEN: AtomicBool = AtomicBool::new(false);

/// Set by `request_open`, consumed by the frame that declares the viewport.
static FOCUS_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Whether `--debug` was on the command line. Closing the window restores this
/// rather than assuming debug should go off.
static CLI_DEBUG: AtomicBool = AtomicBool::new(false);

/// Stick to the newest line unless the user scrolls up to read history. Lives
/// outside the app struct because the viewport callback must be `Fn`.
static FOLLOW_TAIL: AtomicBool = AtomicBool::new(true);

/// The startup banner. Kept because the buffer is emptied every time the
/// window closes, and "when did this session start" has to survive that.
static SESSION_START: OnceLock<String> = OnceLock::new();

fn buffer() -> &'static Mutex<VecDeque<String>> {
    static BUFFER: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    BUFFER.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Records the line the window opens with. Called once, at startup.
pub fn set_session_start(line: &str) {
    let _ = SESSION_START.set(line.to_owned());
}

/// Records the startup `--debug` value so the window can restore it on close.
pub fn set_cli_debug(enabled: bool) {
    CLI_DEBUG.store(enabled, Ordering::Relaxed);
}

pub fn is_open() -> bool {
    OPEN.load(Ordering::Relaxed)
}

/// One line of user-visible log output. Goes to the terminal when there is one
/// and to the window when it is open; with neither, it evaporates, which is the
/// intended silent-launch behavior.
///
/// Called from the message loop and the indicator thread, never from the hook
/// callback.
pub fn emit(line: &str) {
    crate::notify::console_line(line);
    push(line);
}

/// Adds a line to the window only. For messages that already reached the user
/// some other way (a dialog, `eprintln!`) but belong in the transcript too.
pub fn push(line: &str) {
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

/// A line for something the user did: a tray menu pick, a button press. It is
/// prefixed so these stand out among the `[debug]` key-decision lines, which
/// is what makes a log readable when diagnosing "why did that happen".
pub fn action(message: &str) {
    emit(&format!("{} {message}", i18n::t().log_action_prefix));
}

/// Marks the window as wanted. The GUI thread creates it on its next frame;
/// clicking again while it is up only raises it, keeping the transcript.
pub fn request_open() {
    if !OPEN.swap(true, Ordering::SeqCst) {
        if let Ok(mut lines) = buffer().lock() {
            lines.clear();
            if let Some(start) = SESSION_START.get() {
                lines.push_back(start.clone());
            }
            lines.push_back(i18n::t().log_window_hint.to_owned());
        }
        hook::set_debug(true);
    }
    FOCUS_REQUESTED.store(true, Ordering::SeqCst);
}

/// Debug logging goes back to whatever the command line asked for and the
/// buffer is released. Called when the user closes the window and when the
/// whole GUI loop dies.
pub fn on_closed() {
    // Before the flag drops: `push` ignores lines while the window is down, so
    // this is the last moment the closing can be recorded. It still reaches a
    // terminal, which is where a closed window's transcript lives.
    if OPEN.load(Ordering::SeqCst) {
        action(&i18n::action_closed(i18n::t().log_window_title));
    }
    OPEN.store(false, Ordering::SeqCst);
    hook::set_debug(CLI_DEBUG.load(Ordering::Relaxed));
    if let Ok(mut lines) = buffer().lock() {
        lines.clear();
    }
}

/// Declares the log window for this frame. Called from the root viewport's
/// frame; not calling it is what closes the window.
pub fn show_viewport(ctx: &egui::Context) {
    if !OPEN.load(Ordering::Relaxed) {
        return;
    }
    // No icon here: egui would install it as ICON_SMALL only, and
    // `win32::set_window_icons` sets both slots properly (ADR 0038).
    let builder = egui::ViewportBuilder::default()
        .with_title(i18n::t().log_window_title)
        .with_inner_size(theme::LOG_WINDOW);
    ctx.show_viewport_deferred(egui::ViewportId::from_hash_of("winremap-log"), builder, {
        move |ui, _class| {
            let ctx = ui.ctx().clone();
            // Inside the callback the current viewport is this window, so this
            // schedules the log's own repaints. Without it the window would
            // only redraw when the parent re-declares it, which lands each
            // batch of lines a beat late.
            ctx.request_repaint_after(POLL_INTERVAL);
            if FOCUS_REQUESTED.swap(false, Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            // Closing a child viewport is allowed to destroy it: the event
            // loop belongs to the root, so nothing is lost (ADR 0035).
            if ctx.input(|i| i.viewport().close_requested()) {
                on_closed();
            }
            window_ui(ui);
        }
    });
}

fn window_ui(ui: &mut egui::Ui) {
    let texts = i18n::t();
    let mut follow_tail = FOLLOW_TAIL.load(Ordering::Relaxed);

    // Reading the log is the point of this window, so the top holds only what
    // changes how it reads. The two commands live at the bottom, out of the
    // way of the newest line (owner decision 2026-07-21).
    // A filled band with an even margin, so the two ends of the window read
    // as chrome rather than as the first and last lines of the log (owner
    // decision 2026-07-21).
    egui::Panel::top("log-controls")
        .frame(theme::chrome_frame(ui.visuals()))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // No icon: the checkmark is this control's own marker.
                if ui
                    .checkbox(&mut follow_tail, texts.log_window_follow)
                    .changed()
                {
                    FOLLOW_TAIL.store(follow_tail, Ordering::Relaxed);
                }
            });
        });

    egui::Panel::bottom("log-actions")
        .frame(theme::chrome_frame(ui.visuals()))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if icons::button(ui, Icon::Clear, texts.log_window_clear).clicked()
                    && let Ok(mut lines) = buffer().lock()
                {
                    lines.clear();
                }
                if icons::button(ui, Icon::Copy, texts.log_window_copy).clicked()
                    && let Ok(lines) = buffer().lock()
                {
                    let joined = lines.iter().cloned().collect::<Vec<_>>().join("\r\n");
                    ui.ctx().copy_text(joined);
                }
            });
        });

    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical()
            .stick_to_bottom(follow_tail)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Ok(lines) = buffer().lock() else { return };
                for line in lines.iter() {
                    ui.label(egui::RichText::new(line).monospace());
                }
            });
    });
}
