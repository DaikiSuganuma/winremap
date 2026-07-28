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
/// Counts hidden lines too: the mechanics are buffered whether or not the
/// detailed view is showing, so that ticking the box explains what already
/// happened. A remapped keystroke is five or six lines that way, which is
/// why this is not the 5000 it was when only decisions were kept.
const MAX_LINES: usize = 20_000;

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

/// Show the mechanics as well as the decisions (ADR 0057). Off by default:
/// the question the log usually answers is "what did WinRemap do with that
/// key", and one line per key is the shortest way to answer it.
///
/// Every line is buffered either way, so ticking the box shows the detail
/// behind keys already pressed rather than only future ones — which is what
/// makes it useful when something has just gone wrong.
static DETAILED: AtomicBool = AtomicBool::new(false);

/// The startup banner. Kept because the buffer is emptied every time the
/// window closes, and "when did this session start" has to survive that.
static SESSION_START: OnceLock<String> = OnceLock::new();

/// What a line is. Decides how it is drawn, and whether the simple view
/// shows it at all (ADR 0057).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Session banner, hints, warnings — the window's own furniture. Carries
    /// its own wording, so it gets neither a stamp nor a tag.
    Note,
    /// Something the user did: a tray pick, a button.
    Action,
    /// What WinRemap decided about one key. The simple view is these.
    Decision,
    /// The mechanics behind a decision: the physical events that arrived and
    /// the ones WinRemap sent in reply.
    Detail,
}

/// Whether a line belongs in the view the checkbox currently selects.
fn visible(kind: Kind, detailed: bool) -> bool {
    detailed || kind != Kind::Detail
}

fn note(text: String) -> Line {
    Line {
        kind: Kind::Note,
        tag: "",
        at: String::new(),
        text,
    }
}

struct Line {
    kind: Kind,
    /// The gutter tag, already localized. Empty for notes.
    tag: &'static str,
    /// `HH:MM:SS.mmm`. Empty for notes, which carry their own stamp or none.
    at: String,
    text: String,
}

fn buffer() -> &'static Mutex<VecDeque<Line>> {
    static BUFFER: OnceLock<Mutex<VecDeque<Line>>> = OnceLock::new();
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
///
/// Split on newlines, because the window draws one buffer entry as one row of
/// fixed height — the foreground report is three lines of text and would
/// otherwise overflow its row and push every row below it out of place.
pub fn push(line: &str) {
    for part in line.split('\n') {
        push_line(Kind::Note, "", String::new(), part.to_owned());
    }
}

/// A line for something the user did: a tray menu pick, a button press. The
/// tag is what makes these stand out among the key traffic, which is what
/// makes a log readable when diagnosing "why did that happen".
pub fn action(message: &str) {
    let tag = i18n::t().log_action_prefix;
    crate::notify::console_line(&format!("{tag} {message}"));
    push_line(
        Kind::Action,
        tag,
        crate::clock::local_time_of_day(),
        message.to_owned(),
    );
}

/// One line of the debug transcript. Called from the message loop when the
/// hook's event ring is drained (ADR 0016), never from the hook itself.
///
/// The console gets the tag spliced back on, so a terminal session reads the
/// same as the window minus the columns.
pub fn debug_line(kind: Kind, tag: &'static str, text: &str) {
    crate::notify::console_line(&format!("{tag} {text}"));
    push_line(
        kind,
        tag,
        crate::clock::local_time_of_day(),
        text.to_owned(),
    );
}

fn push_line(kind: Kind, tag: &'static str, at: String, text: String) {
    if !OPEN.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut lines) = buffer().lock() {
        if lines.len() >= MAX_LINES {
            lines.pop_front();
        }
        lines.push_back(Line {
            kind,
            tag,
            at,
            text,
        });
    }
}

/// Marks the window as wanted. The GUI thread creates it on its next frame;
/// clicking again while it is up only raises it, keeping the transcript.
pub fn request_open() {
    if !OPEN.swap(true, Ordering::SeqCst) {
        if let Ok(mut lines) = buffer().lock() {
            lines.clear();
            if let Some(start) = SESSION_START.get() {
                lines.push_back(note(start.clone()));
            }
            lines.push_back(note(i18n::t().log_window_hint.to_owned()));
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
    let mut detailed = DETAILED.load(Ordering::Relaxed);

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
                // What the log says, beside how it scrolls: both control how
                // the window reads, which is what the header is for.
                if ui
                    .checkbox(&mut detailed, texts.log_window_detailed)
                    .on_hover_text(texts.log_window_detailed_hint)
                    .changed()
                {
                    DETAILED.store(detailed, Ordering::Relaxed);
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
                // Copies what is on screen, not what is in the buffer: a
                // reader pasting a log into an issue means the one they were
                // just looking at.
                if icons::button(ui, Icon::Copy, texts.log_window_copy).clicked()
                    && let Ok(lines) = buffer().lock()
                {
                    let joined = lines
                        .iter()
                        .filter(|line| visible(line.kind, detailed))
                        .map(as_text)
                        .collect::<Vec<_>>()
                        .join("\r\n");
                    ui.ctx().copy_text(joined);
                }
            });
        });

    egui::CentralPanel::default().show(ui, |ui| {
        let Ok(lines) = buffer().lock() else { return };
        let rows: Vec<&Line> = lines
            .iter()
            .filter(|line| visible(line.kind, detailed))
            .collect();
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        // `both`, and every row laid out with `Extend`: a long macro line
        // scrolls sideways rather than wrapping. Wrapping would cost the
        // column alignment that makes the detailed view readable, and
        // `show_rows` below needs every row to be exactly one row tall.
        egui::ScrollArea::both()
            .stick_to_bottom(follow_tail)
            .auto_shrink([false, false])
            .show_rows(ui, row_height, rows.len(), |ui, range| {
                for i in range {
                    // A stamp equal to the one above it is dropped, so a
                    // column of them reads as groups: everything WinRemap did
                    // in reply to one press shares a moment, and the reply to
                    // the release starts a new one. Repeating the number on
                    // every line hid exactly that (owner request 2026-07-28).
                    let repeated = i > 0 && rows[i - 1].at == rows[i].at;
                    draw_row(ui, rows[i], row_height, repeated);
                }
            });
    });
}

/// One row: when it happened, what kind of line it is, and what it says.
///
/// The three columns are what the flat list was missing. A reader could not
/// tell that the two halves of a remap — the target's press and its release
/// — happened at different moments, because nothing on the line said when.
fn draw_row(ui: &mut egui::Ui, line: &Line, row_height: f32, repeated_stamp: bool) {
    let visuals = ui.visuals();
    let (tag_color, text_color) = match line.kind {
        Kind::Note | Kind::Action => (
            theme::log_action_text(visuals),
            theme::log_action_text(visuals),
        ),
        Kind::Decision => (
            theme::log_weak_text(visuals),
            theme::log_decision_text(visuals),
        ),
        Kind::Detail => (theme::log_weak_text(visuals), theme::log_weak_text(visuals)),
    };
    let time_color = theme::log_weak_text(visuals);
    let row = egui::vec2(ui.available_width(), row_height);
    ui.allocate_ui_with_layout(
        row,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = theme::LOG_COLUMN_GAP;
            // A note is a section marker — the session banner, the hint. It
            // spans from the left edge instead of sitting in the message column,
            // which is what makes it read as a break in the traffic.
            if line.kind != Kind::Note {
                let at = if repeated_stamp { "" } else { line.at.as_str() };
                cell(ui, theme::LOG_TIME_WIDTH, at, time_color);
                cell(ui, theme::LOG_TAG_WIDTH, line.tag, tag_color);
                if line.kind == Kind::Detail {
                    ui.add_space(theme::LOG_DETAIL_INDENT);
                }
            }
            // The message takes its natural width rather than what is left of
            // the window: that is what tells the scroll area the content is
            // wider than the viewport, and so what puts a horizontal
            // scrollbar under a long macro line instead of hiding its tail.
            label(ui, &line.text, text_color);
        },
    );
}

/// A fixed-width column, for the two that have to line up down the page.
fn cell(ui: &mut egui::Ui, width: f32, text: &str, color: egui::Color32) {
    let height = ui.available_height();
    let size = egui::vec2(width, height);
    ui.allocate_ui_with_layout(
        size,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| label(ui, text, color),
    );
}

/// Monospace so the key names align, `Extend` so nothing wraps out of its row
/// — `show_rows` needs every row to be exactly one row tall.
fn label(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.add(
        egui::Label::new(egui::RichText::new(text).monospace().color(color))
            .wrap_mode(egui::TextWrapMode::Extend),
    );
}

/// The row as one line of text, for the clipboard. Every line keeps its
/// stamp, unlike the window, which drops repeats: a pasted log has no row
/// above it to inherit from.
fn as_text(line: &Line) -> String {
    match line.kind {
        Kind::Note => line.text.clone(),
        Kind::Detail => format!("{} {} {}", line.at, line.tag, indent(&line.text)),
        _ => format!("{} {} {}", line.at, line.tag, line.text),
    }
}

/// The clipboard's stand-in for the indent the window draws.
fn indent(text: &str) -> String {
    format!("  {text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: Kind, tag: &'static str, text: &str) -> Line {
        Line {
            kind,
            tag,
            at: "15:42:07.318".to_owned(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn the_simple_view_keeps_everything_but_the_mechanics() {
        for kind in [Kind::Note, Kind::Action, Kind::Decision] {
            assert!(
                visible(kind, false),
                "{kind:?} must survive the simple view"
            );
        }
        assert!(!visible(Kind::Detail, false));
    }

    #[test]
    fn the_detailed_view_keeps_every_kind() {
        for kind in [Kind::Note, Kind::Action, Kind::Decision, Kind::Detail] {
            assert!(
                visible(kind, true),
                "{kind:?} must survive the detailed view"
            );
        }
    }

    /// The clipboard has no columns, so it has to carry the stamp and the tag
    /// as text — someone pasting a log into an issue is pasting the evidence
    /// that two lines happened at different moments.
    #[test]
    fn copied_lines_carry_the_stamp_and_the_tag() {
        assert_eq!(
            as_text(&line(Kind::Decision, "[判定]", "C-n → Down に置換")),
            "15:42:07.318 [判定] C-n → Down に置換"
        );
        assert_eq!(
            as_text(&line(Kind::Detail, "[注入]", "Down ↓（置換）")),
            "15:42:07.318 [注入]   Down ↓（置換）"
        );
    }

    /// A note is the window's own furniture — the session banner carries its
    /// own full stamp, so a second one would be noise.
    #[test]
    fn copied_notes_stay_as_they_were_written() {
        assert_eq!(
            as_text(&note("WinRemap v0.5.0".to_owned())),
            "WinRemap v0.5.0"
        );
    }
}
