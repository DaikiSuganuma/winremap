//! IME status indicator: shows a translucent panel in the center of the
//! active window the moment the IME turns on (docs/08_ime-indicator-design.md,
//! ADR 0020).
//!
//! This is a self-contained feature, deliberately isolated from the remapping
//! core: the only touch points are one notification call each in hook.rs,
//! window.rs, tray.rs, and main.rs. All work happens on a dedicated
//! "indicator thread" owning the overlay window; the hook only posts it a
//! bounded, non-blocking thread message (invariant 2, explicit exception 3).
//!
//! This file is orchestration only and stays `unsafe`-free (invariant 3):
//! the Win32 calls live in `detect` (IME query) and `overlay` (window,
//! timers, thread messaging).

mod detect;
mod overlay;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use windows::Win32::UI::WindowsAndMessaging::{WM_APP, WM_TIMER};

use crate::i18n;
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

/// Indicator thread id; 0 while not running. Written by the main thread and
/// the dying thread itself, read (wait-free) from the hook callback.
static THREAD_ID: AtomicU32 = AtomicU32::new(0);
/// Kept so stop() can join. Only touched from the main thread.
static THREAD_HANDLE: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

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
        .is_some_and(|table| table.ime_indicator.enabled)
}

/// Hook-callback touch point. Hook-safe by construction: wait-free loads, a
/// borrow-only chord comparison (no allocation), and at most one
/// `PostThreadMessageW` (AGENTS.md invariant 2, exception 3 / ADR 0020).
/// The key itself always passes through; this never influences remapping.
pub fn notify_keydown(input: KeyCombo) {
    let tid = THREAD_ID.load(Ordering::Relaxed);
    if tid == 0 {
        return;
    }
    let table = crate::hook::REMAP_TABLE.load();
    let Some(table) = table.as_ref() else {
        return;
    };
    let settings = &table.ime_indicator;
    if !settings.enabled {
        return;
    }
    // Built-in VK candidates match regardless of held modifiers; configured
    // triggers (e.g. "C-Space") match on the full chord (ADR 0021).
    if !is_toggle_candidate(input.vk) && !settings.trigger_keys.contains(&input) {
        return;
    }
    overlay::post_to_thread(tid, MSG_TOGGLE_POKE);
}

/// Foreground-change touch point; runs on the main thread's WinEvent
/// callback (not the keyboard hook), but is kept just as cheap.
pub fn notify_foreground_changed() {
    let tid = THREAD_ID.load(Ordering::Relaxed);
    if tid == 0 || !indicator_enabled() {
        return;
    }
    overlay::post_to_thread(tid, MSG_FOREGROUND);
}

/// Starts or updates the feature to match the loaded config. Called at
/// startup and after every reload (tray). Never started → disabled configs
/// cost nothing: no thread, no messages, no window.
pub fn sync_with_config() {
    let tid = THREAD_ID.load(Ordering::Acquire);
    if tid != 0 {
        overlay::post_to_thread(tid, MSG_SETTINGS);
    } else if current_settings().enabled {
        start_thread();
    }
}

/// Stops the indicator thread if it runs. Called once at shutdown.
pub fn stop() {
    let tid = THREAD_ID.swap(0, Ordering::AcqRel);
    if tid != 0 {
        overlay::post_quit_to(tid);
    }
    if let Ok(mut handle) = THREAD_HANDLE.lock()
        && let Some(handle) = handle.take()
    {
        let _ = handle.join();
    }
}

fn start_thread() {
    let (ready_tx, ready_rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("ime-indicator".into())
        .spawn(move || thread_main(&ready_tx));
    let handle = match spawned {
        Ok(handle) => handle,
        Err(e) => {
            eprintln!("{}", i18n::ime_indicator_failed(&e.to_string()));
            return;
        }
    };
    match ready_rx.recv() {
        Ok(tid) => {
            THREAD_ID.store(tid, Ordering::Release);
            if let Ok(mut slot) = THREAD_HANDLE.lock()
                && let Some(old) = slot.replace(handle)
            {
                // A previous thread that died (panic) was never joined.
                let _ = old.join();
            }
        }
        // The thread reported its own failure before dropping the sender.
        Err(_) => {
            let _ = handle.join();
        }
    }
}

fn thread_main(ready: &mpsc::Sender<u32>) {
    let overlay = match overlay::Overlay::create() {
        Ok(overlay) => overlay,
        Err(e) => {
            eprintln!("{}", i18n::ime_indicator_failed(&e.to_string()));
            return; // ready is dropped unsent; start_thread sees the Err
        }
    };
    let _ = ready.send(overlay::current_thread_id());
    // A panic must stay inside this feature: the remap hook keeps running
    // (design doc §5, failure isolation).
    let outcome = catch_unwind(AssertUnwindSafe(|| run(&overlay)));
    THREAD_ID.store(0, Ordering::Release);
    if outcome.is_err() {
        eprintln!(
            "{}",
            i18n::ime_indicator_failed("indicator thread panicked")
        );
    }
}

/// The indicator state machine. Triggers arm a short one-shot timer (both
/// debouncing bursts and letting the IME settle), then a single query
/// decides: show on "became ON" or "still ON in a newly focused window",
/// hide as soon as OFF is observed.
fn run(overlay: &overlay::Overlay) {
    let mut last_on = false;
    let mut last_target: isize = 0;
    let mut query_timer: usize = 0;
    while let Some((message, wparam)) = overlay::next_thread_message() {
        match message {
            MSG_TOGGLE_POKE | MSG_FOREGROUND => {
                if query_timer != 0 {
                    overlay::kill_thread_timer(query_timer);
                }
                query_timer = overlay::set_thread_timer(QUERY_DELAY_MS);
            }
            WM_TIMER if wparam == query_timer && query_timer != 0 => {
                overlay::kill_thread_timer(query_timer);
                query_timer = 0;
                let settings = current_settings();
                if !settings.enabled {
                    overlay.hide();
                    last_on = false;
                    continue;
                }
                let sample = detect::query_foreground();
                if sample.shell_surface {
                    // Taskbar/desktop clicks: never flash over the shell
                    // itself (ADR 0023), but forget the last target so that
                    // refocusing the previous app re-flashes the panel when
                    // its IME is still on (ADR 0026).
                    last_target = 0;
                    if crate::hook::debug_enabled() {
                        println!("{}", i18n::t().debug_ime_shell_skip);
                    }
                    continue;
                }
                let is_on = sample.open == Some(true);
                let shown = is_on && (!last_on || sample.target != last_target);
                if shown {
                    // Exe name, not the window title: titles can carry
                    // sensitive document names and churn constantly
                    // (ADR 0024).
                    let label = settings
                        .show_app_name
                        .then(|| crate::window::app_display_name(sample.target))
                        .flatten();
                    overlay.show(sample.target, &settings, label.as_deref());
                } else if !is_on {
                    // Also covers "unknown" (query failed): prefer not
                    // showing over showing wrongly (design doc §3.2).
                    overlay.hide();
                }
                if crate::hook::debug_enabled() {
                    // This thread is not the hook: printing here is fine.
                    println!(
                        "{}",
                        i18n::debug_ime_query(sample.open, shown, sample.via_core_window)
                    );
                }
                last_on = is_on;
                last_target = sample.target;
            }
            MSG_SETTINGS if !current_settings().enabled => {
                overlay.hide();
                last_on = false;
            }
            _ => {}
        }
    }
}
