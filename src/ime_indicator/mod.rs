//! IME status indicator: shows a translucent panel in the center of the
//! active window the moment the IME turns on (docs/v0.1/05_ime-indicator-design.md,
//! ADR 0020).
//!
//! This is a self-contained feature, deliberately isolated from the remapping
//! core: the only touch points are one notification call each in hook/,
//! window.rs, tray.rs, and main.rs. All work happens on a dedicated
//! "indicator thread" owning the overlay window; the hook only posts it a
//! bounded, non-blocking thread message (invariant 2, explicit exception 3).
//!
//! This file is orchestration only and stays `unsafe`-free (invariant 3):
//! the Win32 calls live in `detect` (IME query), `overlay` (the window and
//! its timers) and `crate::thread_msg` (thread messages); the thread's
//! lifecycle is `crate::worker` (ADR 0085).

mod decide;
mod detect;
mod overlay;
#[cfg(test)]
mod tests;

use windows::Win32::UI::WindowsAndMessaging::{WM_APP, WM_TIMER};

use crate::i18n;
use crate::thread_msg;
use crate::worker::Worker;
use decide::{Last, Panel, Verdict, decide};
use winremap::ime_indicator_settings::IndicatorSettings;
use winremap::keymap::KeyCombo;

/// A toggle-candidate key went down; query the IME state shortly.
const MSG_TOGGLE_POKE: u32 = WM_APP + 0x21;
/// The foreground window changed; query and show if the IME is on there.
const MSG_FOREGROUND: u32 = WM_APP + 0x22;
/// The config was reloaded; re-read settings and hide if now disabled.
const MSG_SETTINGS: u32 = WM_APP + 0x23;

/// Wait between a trigger and the actual query, absorbing the time the IME
/// needs to process the toggle key itself (design doc §3.1).
const QUERY_DELAY_MS: u32 = 50;

/// The indicator thread. Its id is 0 while it is not running, which is what
/// the hook reads (wait-free) to decide whether there is anyone to poke.
static THREAD: Worker = Worker::new();

/// IME toggle candidates (design doc §3.1): VK_KANA 0x15, VK_IME_ON 0x16,
/// VK_KANJI 0x19, VK_IME_OFF 0x1A, VK_CONVERT 0x1C, VK_NONCONVERT 0x1D, and
/// the JIS Zenkaku/Hankaku pair VK_OEM_AUTO 0xF3 / VK_OEM_ENLW 0xF4.
const fn is_toggle_candidate(vk: u16) -> bool {
    matches!(vk, 0x15 | 0x16 | 0x19 | 0x1A | 0x1C | 0x1D | 0xF3 | 0xF4)
}

/// Owned snapshot for the indicator thread. NOT hook-safe: cloning the
/// trigger list allocates, so hook-side checks below borrow instead.
fn current_settings() -> IndicatorSettings {
    crate::hook::REMAP_TABLE
        .load()
        .as_ref()
        .map(|table| table.ime_indicator.clone())
        .unwrap_or_default()
}

fn indicator_enabled() -> bool {
    crate::hook::REMAP_TABLE
        .load()
        .as_ref()
        .is_some_and(|table| feature_wanted(&table.ime_indicator))
}

/// Whether anything at all depends on knowing the IME state. The panel and
/// the tinted cursor (ADR 0067) are two displays of one observation, and
/// either of them alone is reason enough to run the thread.
fn feature_wanted(settings: &IndicatorSettings) -> bool {
    settings.enabled || settings.change_cursor_color
}

/// Hook-callback touch point. Hook-safe by construction: wait-free loads, a
/// borrow-only chord comparison (no allocation), and at most one
/// `PostThreadMessageW` (AGENTS.md invariant 2, exception 3 / ADR 0020).
/// The key itself always passes through; this never influences remapping.
pub fn notify_keydown(input: KeyCombo) {
    let tid = THREAD.thread_id();
    if tid == 0 {
        return;
    }
    let table = crate::hook::REMAP_TABLE.load();
    let Some(table) = table.as_ref() else {
        return;
    };
    let settings = &table.ime_indicator;
    // The cursor tint needs this poke as much as the panel does: the IME
    // toggle key is the only signal that the state changed without the
    // foreground window changing (ADR 0067).
    if !feature_wanted(settings) {
        return;
    }
    // Built-in VK candidates match regardless of held modifiers; configured
    // triggers (e.g. "C-Space") match on the full chord (ADR 0021).
    if !is_toggle_candidate(input.vk) && !settings.trigger_keys.contains(&input) {
        return;
    }
    thread_msg::post_to_thread(tid, MSG_TOGGLE_POKE, 0);
}

/// Foreground-change touch point; runs on the main thread's WinEvent
/// callback (not the keyboard hook), but is kept just as cheap.
pub fn notify_foreground_changed() {
    let tid = THREAD.thread_id();
    if tid == 0 || !indicator_enabled() {
        return;
    }
    thread_msg::post_to_thread(tid, MSG_FOREGROUND, 0);
}

/// Starts or updates the feature to match the loaded config. Called at
/// startup and after every reload (tray). Never started → disabled configs
/// cost nothing: no thread, no messages, no window.
pub fn sync_with_config() {
    let tid = THREAD.thread_id();
    if tid != 0 {
        thread_msg::post_to_thread(tid, MSG_SETTINGS, 0);
    } else if feature_wanted(&current_settings()) {
        THREAD.start("ime-indicator", overlay::Overlay::create, run, report);
    }
    // Whatever the config now says, a tint left over from the previous one
    // is not it. Also the unconditional restore of ADR 0067 decision 5 on
    // the startup call: nothing has to be remembered across runs.
    if !current_settings().change_cursor_color {
        crate::cursor::restore();
    }
}

/// Stops the indicator thread if it runs, and puts the cursor back. Called
/// once at shutdown — leaving a tinted cursor behind on a clean exit would
/// make the one signal this feature has ("tinted and no tray icon means
/// WinRemap died") a lie (ADR 0067).
pub fn stop() {
    crate::cursor::restore();
    THREAD.stop();
}

/// What the thread could not do — build its window, start, or keep running
/// — said where the user can see it.
fn report(error: &str) {
    crate::notify::error(&i18n::ime_indicator_failed(error));
}

/// The indicator state machine. Triggers arm a short one-shot timer (both
/// debouncing bursts and letting the IME settle), then a single query
/// decides: show on "became ON" or "still ON in a newly focused window",
/// hide as soon as OFF is observed. What the query means is `decide`; this
/// loop is the timer, the Win32 calls and the log lines around it.
fn run(overlay: &overlay::Overlay) {
    let mut last = Last::default();
    let mut query_timer: usize = 0;
    while let Some((message, wparam)) = thread_msg::next_thread_message() {
        match message {
            MSG_TOGGLE_POKE | MSG_FOREGROUND => {
                if query_timer != 0 {
                    thread_msg::kill_thread_timer(query_timer);
                }
                query_timer = thread_msg::set_thread_timer(QUERY_DELAY_MS);
            }
            WM_TIMER if wparam == query_timer && query_timer != 0 => {
                thread_msg::kill_thread_timer(query_timer);
                query_timer = 0;
                let settings = current_settings();
                if !feature_wanted(&settings) {
                    overlay.hide();
                    crate::cursor::apply(false, settings.cursor_color);
                    last.on = false;
                    continue;
                }
                let sample = detect::query_foreground();
                let (verdict, next) = decide(&sample, &settings, last);
                last = next;
                let (panel, cursor) = match verdict {
                    Verdict::ShellSurface => {
                        if crate::hook::debug_enabled() {
                            crate::gui::log::tagged(
                                crate::gui::log::Kind::Detail,
                                i18n::t().log_tag_ime,
                                i18n::t().debug_ime_shell_skip,
                            );
                        }
                        continue;
                    }
                    Verdict::Query { panel, cursor } => (panel, cursor),
                };
                // The cursor before the panel, as it has always been: it is a
                // status light and has to be right when focus moves between
                // applications whose IME states differ. v0.8 acceptance M-1,
                // 2026-08-04: composing, moving the pointer out of the window
                // and confirming left the cursor plain with the IME still on
                // (ADR 0072), which is why `decide` only moves it on a known
                // state.
                if let Some(open) = cursor {
                    let action = crate::cursor::apply(open, settings.cursor_color);
                    if crate::hook::debug_enabled() {
                        // Not the hook thread: logging here is allowed. The
                        // line is what tells M-2 apart from a quiet moment —
                        // this runs on every foreground change, so "again"
                        // is the common case and has to be visible.
                        crate::gui::log::tagged(
                            crate::gui::log::Kind::Detail,
                            i18n::t().log_tag_ime,
                            &i18n::debug_cursor_action(action),
                        );
                    }
                }
                match panel {
                    Panel::Show => {
                        // Exe name, not the window title: titles can carry
                        // sensitive document names and churn constantly
                        // (ADR 0024).
                        let label = settings
                            .show_app_name
                            .then(|| crate::window::app_display_name(sample.target))
                            .flatten();
                        overlay.show(sample.target, &settings, label.as_deref());
                    }
                    Panel::Hide => overlay.hide(),
                    Panel::Keep => {}
                }
                if crate::hook::debug_enabled() {
                    // This thread is not the hook: logging here is fine.
                    // Detail, because the indicator polls on every focus
                    // change and on every trigger key — a stream of its own,
                    // which the simple view is not about.
                    crate::gui::log::tagged(
                        crate::gui::log::Kind::Detail,
                        i18n::t().log_tag_ime,
                        &i18n::debug_ime_query(sample.open, panel == Panel::Show, sample.via_child),
                    );
                }
            }
            MSG_SETTINGS => {
                let settings = current_settings();
                if !settings.enabled {
                    overlay.hide();
                    last.on = false;
                }
                if !settings.change_cursor_color {
                    crate::cursor::apply(false, settings.cursor_color);
                }
            }
            _ => {}
        }
    }
}
