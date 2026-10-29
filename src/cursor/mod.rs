//! Tinting the mouse cursor while the IME is on (ADR 0067).
//!
//! `SetSystemCursor` replaces a cursor **for the whole session**, not just
//! for WinRemap's own windows — which is what makes it useful (it shows over
//! every application, including elevated ones, where an overlay window
//! cannot reach) and also what makes it dangerous: the replacement outlives
//! this process. Everything here is arranged around that one fact:
//!
//! - the cursor is only replaced while the IME is **on**, so what is left
//!   behind after a crash is always the tinted one — never a normal-looking
//!   cursor that quietly is not ours (ADR 0067 decision 4);
//! - [`restore`] is cheap and idempotent, so it can be called at startup
//!   without knowing how the last run ended (decision 5);
//! - [`install_crash_restore`] covers the ends that still run code.
//!
//! A sign-out needs nothing: the replacement lives in the session's window
//! station and Windows loads cursors afresh from the registry at logon.
//!
//! The tint keeps the cursor the user actually has — theme, size and shape —
//! and only recolours it: each pixel's brightness becomes the same fraction
//! of the chosen colour, so a white body takes the colour. A white border is
//! then drawn around the shape, because the colour alone is darker than the
//! white it replaced and would be lost on a dark application (see
//! [`outlined`]).
//!
//! A cursor that comes back empty at this display's scale is read and built
//! again from a **DPI-unaware** thread, and the reload from the registry
//! always is: on a scaled display, what a DPI-aware process is handed for the
//! I-beam has nothing in it at all (see [`Unscaled`], ADR 0076).

//!
//! Split by what each file touches (ADR 0084): this file owns the session's
//! cursor table — what is installed, what was there before, and the public
//! API; `scale` is the DPI context; `tint` reads a cursor and builds its
//! recoloured copy; `gdi` is the bitmap plumbing; `pixels` is the arithmetic
//! and calls no Win32 at all.

mod gdi;
mod pixels;
mod scale;
#[cfg(test)]
mod tests;
mod tint;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::System::Diagnostics::Debug::{EXCEPTION_POINTERS, SetUnhandledExceptionFilter};
use windows::Win32::UI::WindowsAndMessaging::{
    CopyIcon, HCURSOR, HICON, IDC_ARROW, IDC_IBEAM, OCR_IBEAM, OCR_NORMAL, SPI_SETCURSORS,
    SPIF_SENDCHANGE, SYSTEM_CURSOR_ID, SetSystemCursor, SystemParametersInfoW,
};
use windows::core::PCWSTR;

use scale::Unscaled;
use tint::{snapshot, tinted};

/// The cursors WinRemap replaces. The arrow alone would leave the state
/// invisible exactly when it matters: while typing, what is under the
/// pointer is usually a text field, and that shows the I-beam.
const REPLACED: [(SYSTEM_CURSOR_ID, PCWSTR); 2] = [(OCR_NORMAL, IDC_ARROW), (OCR_IBEAM, IDC_IBEAM)];

/// Whether the tinted cursors are currently installed. Read from the crash
/// paths, so it is an atomic rather than part of the mutex below.
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Tinted copies, built once per colour. `isize` because a raw `HCURSOR` is
/// not `Send`; the handles are process-wide and used from one thread.
static TINTED: Mutex<Option<Prepared>> = Mutex::new(None);

/// Copies of the cursors as they were before this run touched anything —
/// what [`restore`] puts back (ADR 0073 decision 1). Taken once by
/// [`capture_pristine`]; empty if that could not be done, which is a
/// supported state (decision 2) and not an error to keep reporting.
static PRISTINE: Mutex<Vec<(SYSTEM_CURSOR_ID, isize)>> = Mutex::new(Vec::new());

struct Prepared {
    color: (u8, u8, u8),
    cursors: Vec<(SYSTEM_CURSOR_ID, isize)>,
    /// The ones that could not be built for this colour, and why.
    ///
    /// Remembered rather than only reported, because the build runs **once
    /// per colour** — normally within seconds of startup. v0.9 acceptance,
    /// M-2, 2026-08-13: the I-beam failed to build in that window, and by the
    /// time the log window was opened twenty minutes later the line saying so
    /// had never been stored ([`crate::gui::log`] drops what it is given
    /// while no window is open). The tint then ran half-installed for the
    /// rest of the run with nothing anywhere to say why. Kept so
    /// [`apply`] can say it again to whoever is looking now.
    missing: Vec<Trouble>,
}

/// Something here went wrong in a way that used to be dropped on the floor.
///
/// **The reason this type exists is that `1814` was returned on every single
/// `restore()` since the feature shipped and nobody could know** — the call
/// sites wrote `continue` and `let _ =` (ADR 0073 decision 5). Each variant
/// is a thing that, when it happens, explains something the owner can see.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trouble {
    /// No pristine copy could be taken for this cursor, so restoring it falls
    /// back to `SPI_SETCURSORS` alone (decision 2).
    NoSnapshot {
        id: SYSTEM_CURSOR_ID,
        why: &'static str,
    },
    /// The pristine copy could not be put back.
    RestoreFailed {
        id: SYSTEM_CURSOR_ID,
        why: &'static str,
    },
    /// `SPI_SETCURSORS` itself failed — the last line of defence against a
    /// tint outliving the run that installed it.
    ReloadFailed,
    /// The cursor being tinted has nothing drawn in it. Tinting it would
    /// produce another empty one, which is how M-2 kept reproducing itself
    /// (decision 4).
    SourceEmpty { id: SYSTEM_CURSOR_ID },
    /// The tint came out with no drawn pixel at all and was thrown away
    /// rather than installed — the symptom the owner reported four times
    /// (decision 3).
    ResultEmpty { id: SYSTEM_CURSOR_ID },
    /// The tint could not be built.
    BuildFailed {
        id: SYSTEM_CURSOR_ID,
        why: &'static str,
    },
}

/// Puts one [`Trouble`] where it can be read.
///
/// `emit` rather than `tagged`, for two reasons: these are warnings, which is
/// what that channel is for, and they must **not** be hidden behind the
/// detailed view — a line saying the tint was thrown away is the answer to
/// "why is my cursor not coloured", and the simple view is where that gets
/// looked for.
///
/// Never called from the crash paths: `record` takes a lock, and a process
/// on its way down through an unhandled exception is the wrong place to want
/// one. See [`restore_inner`].
fn report(trouble: Trouble) {
    crate::gui::log::emit(&crate::i18n::debug_cursor_trouble(trouble));
}

/// Takes the copy that every later [`restore`] is made from, and must be
/// called **once, at startup, before anything of ours can tint** (ADR 0073
/// decision 1).
///
/// The version this replaced asked `user32.dll` for its built-in cursors, on
/// the assumption that an `IDC_*` atom doubles as that cursor's resource id
/// inside the module. **That assumption does not hold on Windows 11 26200**:
/// the call returns `1814` (`ERROR_RESOURCE_NAME_NOT_FOUND`) every time, and
/// it had been doing so since the feature shipped, silently, because the
/// failure was `continue`d past. Restoring therefore ran on `SPI_SETCURSORS`
/// alone — and `SPI_SETCURSORS` reloads from `HKCU\Control Panel\Cursors`,
/// which on a machine using the stock scheme is empty. The two-step design of
/// ADR 0067 decision 5 had one working step.
///
/// So the pristine cursors are taken from the only place that is known to
/// have them: the session, at the moment this process has not yet changed
/// anything.
///
/// Two things are done about the run that came before:
///
/// - **`SPI_SETCURSORS` first.** A run killed while tinted left its tint in
///   the session and it is still there now; copying it would make every later
///   restore put the tint back. This call is measured to clear it (2026-08-09).
/// - **Then each copy is checked to have something drawn in it**, and dropped
///   if it does not. An empty cursor is exactly what must never become the
///   thing "restore" means.
pub fn capture_pristine() {
    reload_from_registry(true);
    let mut taken = Vec::new();
    for (id, name) in REPLACED {
        match snapshot(name) {
            Ok(icon) => taken.push((id, icon.0 as isize)),
            Err(why) => report(Trouble::NoSnapshot { id, why }),
        }
    }
    // Said even when it all worked, which is the point. The old first step
    // failed on every call for two versions and left no trace, so "the
    // safety net is armed" was not a thing anyone could look up — only its
    // absence, and only by noticing a cursor that stayed wrong.
    crate::gui::log::emit(&crate::i18n::debug_cursor_snapshot(
        taken.len(),
        REPLACED.len(),
    ));
    if let Ok(mut pristine) = PRISTINE.lock() {
        *pristine = taken;
    }
}

/// Undoes any replacement — ours, or a leftover from a previous run that died
/// before it could undo its own.
///
/// Two steps, and both are needed:
///
/// 1. **Put back the copies [`capture_pristine`] took.** This is the step that
///    works on a machine using the stock scheme, where the registry has
///    nothing for `SPI_SETCURSORS` to reload and a replacement would simply
///    stay.
/// 2. **Then `SPI_SETCURSORS`.** A user with a cursor scheme of their own has
///    those paths in the registry, and this overwrites step 1 with them — so
///    the copies from step 1 are what remains when there was no scheme to
///    restore. It is also what picks up a scheme the user changed *while*
///    WinRemap was running, which the copies cannot know about.
///
/// Cheap and idempotent, which is what lets startup call it without knowing
/// how the last run ended (ADR 0067 decision 5). Restoring the cursor that is
/// being replaced is not an option: `SetSystemCursor` **destroys** what it is
/// given, which is why this is a copy taken beforehand rather than one kept
/// at replacement time.
pub fn restore() {
    restore_inner(true);
}

/// `report` says whether the [`Trouble`] channel may be used. The crash paths
/// pass `false`: writing a log line takes a lock, and a process dying inside
/// an unhandled exception filter is the wrong place to want one — putting
/// the cursor back is worth more there than saying so.
fn restore_inner(report_trouble: bool) {
    INSTALLED.store(false, Ordering::Relaxed);
    if let Ok(pristine) = PRISTINE.lock() {
        for (id, icon) in pristine.iter() {
            // A copy per call, for the reason in the doc comment: what is
            // handed over is destroyed, and this handle has to survive every
            // later restore.
            // SAFETY: the stored handles come from `snapshot` and are alive
            // for the life of the process.
            let Ok(copy) = (unsafe { CopyIcon(HICON(*icon as *mut _)) }) else {
                if report_trouble {
                    report(Trouble::RestoreFailed {
                        id: *id,
                        why: "CopyIcon",
                    });
                }
                continue;
            };
            // SAFETY: `copy` is live and `id` is a documented OCR_* constant.
            if unsafe { SetSystemCursor(HCURSOR(copy.0), *id) }.is_err() && report_trouble {
                report(Trouble::RestoreFailed {
                    id: *id,
                    why: "SetSystemCursor",
                });
            }
        }
    }
    reload_from_registry(report_trouble);
}

/// Step 2 of a restore, on its own because startup needs it before there is
/// anything to restore from.
fn reload_from_registry(report_trouble: bool) {
    // Unscaled for the same reason the reads are (ADR 0076), and here it is
    // not only this process that is affected: from a DPI-aware thread this
    // call puts the **empty** scaled I-beam into the session's cursor table,
    // where the next `snapshot` then finds nothing to copy — so the step that
    // exists to undo a replacement was quietly disabling the one that undoes
    // it. Measured 2026-08-15: aware leaves an all-transparent I-beam
    // registered, unaware leaves the machine's own.
    let _unscaled = Unscaled::enter();
    // SAFETY: SPI_SETCURSORS takes no input buffer; the null pointer and 0
    // are what the documentation prescribes for it.
    if unsafe { SystemParametersInfoW(SPI_SETCURSORS, 0, None, SPIF_SENDCHANGE) }.is_err()
        && report_trouble
    {
        report(Trouble::ReloadFailed);
    }
}

/// What one [`apply`] call did to the session's cursors, so the debug log can
/// say it.
///
/// v0.8 acceptance M-2 (2026-08-08): the tinted I-beam goes invisible now and
/// then, and the log could not distinguish "the tint was put on again" from
/// "nothing happened" — the caller runs on every foreground change and every
/// trigger key, so both are common. Three explanations were measured and
/// ruled out that day (a handle another process holds is not invalidated;
/// `SPI_SETCURSORS` with an empty registry value does not blank the I-beam;
/// repeated `SetSystemCursor` does not damage the contents), which leaves
/// the question of what the call sequence was when it broke.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Action {
    pub kind: Kind,
    /// The tinted copies were built here (first use, or the colour changed).
    pub rebuilt: bool,
    /// Cursors handed to `SetSystemCursor`.
    pub replaced: u8,
    /// Everything [`REPLACED`] names that did not get installed, **however it
    /// went missing** — derived from `replaced` rather than counted up from
    /// the failures seen along the way.
    ///
    /// It was counted up until v0.9, and that is how a half-installed tint
    /// came to read as a success: a cursor whose tint could not be built was
    /// dropped from the prepared set by a `filter_map`, so the install loop
    /// never saw it and never counted it. The log said `2 replaced` when both
    /// went on and `1 replaced` — with no failure — when one did. Deriving it
    /// cannot miss a path, because it does not know about paths.
    pub failed: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The IME is off and no tint was installed: nothing was called.
    Idle,
    /// The tint was taken off.
    Restored,
    /// The tint went on where there was none.
    Installed,
    /// `SetSystemCursor` ran for a tint that was **already installed**. This
    /// is what a foreground change between two applications with the IME on
    /// takes, and it is the path M-2 is suspected of.
    Reinstalled,
    /// The IME is on and **not one cursor was replaced**: every tint either
    /// failed to build or was rejected for being empty. Says out loud what
    /// used to read as a successful install of nothing (ADR 0073 decision 3
    /// — an empty tint is now dropped, and dropping it silently would move
    /// the same blind spot one step along).
    Failed,
}

/// Installs or removes the tint. `on` is the IME's open state.
///
/// Building the tinted copies is done once per colour and reused, because
/// this runs on every IME toggle.
pub fn apply(on: bool, color: (u8, u8, u8)) -> Action {
    let idle = Action {
        kind: Kind::Idle,
        rebuilt: false,
        replaced: 0,
        failed: 0,
    };
    if !on {
        if INSTALLED.swap(false, Ordering::Relaxed) {
            restore();
            return Action {
                kind: Kind::Restored,
                ..idle
            };
        }
        return idle;
    }
    let Ok(mut slot) = TINTED.lock() else {
        return idle;
    };
    let was_installed = INSTALLED.load(Ordering::Relaxed);
    let mut rebuilt = false;
    if slot.as_ref().is_none_or(|p| p.color != color) {
        // Rebuilt from the system's cursors, so it has to happen while ours
        // are not installed — otherwise the tint would be tinted again.
        if INSTALLED.load(Ordering::Relaxed) {
            restore();
        }
        let mut cursors = Vec::new();
        let mut missing = Vec::new();
        for (id, name) in &REPLACED {
            match tinted(*id, *name, color) {
                Ok(cur) => cursors.push((*id, cur.0 as isize)),
                Err(trouble) => {
                    report(trouble);
                    missing.push(trouble);
                }
            }
        }
        // Nothing built means nothing to remember. Caching the empty result
        // would keep this colour un-tintable until the config changed it,
        // whereas the failures worth catching are the transient ones — M-2
        // came and went four times in normal use.
        *slot = (!cursors.is_empty()).then_some(Prepared {
            color,
            cursors,
            missing,
        });
        rebuilt = true;
    }
    let Some(prepared) = slot.as_ref() else {
        return outcome(0, rebuilt, was_installed);
    };
    // Said again on every fresh install, because once was not enough: the
    // build happens at most once per colour and its report can land while
    // nothing is listening. A toggle of the IME is a deliberate act a few
    // times a minute at most, and this only speaks at all while the tint is
    // genuinely incomplete — so "open the log and switch the IME on" is now
    // a way to ask why. Not on `Reinstalled`: that runs on every foreground
    // change, which would turn an explanation into noise (ADR 0016).
    if !was_installed {
        for trouble in &prepared.missing {
            report(*trouble);
        }
    }
    let mut replaced = 0u8;
    for (id, cursor) in &prepared.cursors {
        // A copy per call: SetSystemCursor takes ownership and destroys what
        // it is given, so handing it the stored handle would leave the
        // second toggle with a dangling one.
        // SAFETY: the stored handles come from CreateIconIndirect below and
        // are alive for the life of the process.
        let Ok(copy) = (unsafe { CopyIcon(HICON(*cursor as *mut _)) }) else {
            report(Trouble::BuildFailed {
                id: *id,
                why: "CopyIcon",
            });
            continue;
        };
        // SAFETY: `copy` is a live cursor of the system's own size; `id` is
        // one of the documented OCR_* constants. Ownership passes here.
        match unsafe { SetSystemCursor(HCURSOR(copy.0), *id) } {
            Ok(()) => replaced += 1,
            Err(_) => report(Trouble::BuildFailed {
                id: *id,
                why: "SetSystemCursor",
            }),
        }
    }
    // Only claim the tint is on if some of it went on. Claiming it wrongly
    // has a cost in each direction: too eager and the next "IME off" runs a
    // restore for a tint that is not there, too shy and a tint that *is*
    // there never gets taken off. `was_installed` keeps the second case
    // right when a re-install fails.
    INSTALLED.store(was_installed || replaced > 0, Ordering::Relaxed);
    outcome(replaced, rebuilt, was_installed)
}

/// What to report for an [`apply`] that installed `replaced` of [`REPLACED`].
///
/// Pure, and separate from `apply`, so the one thing that went wrong in v0.9
/// can be tested: a tint that only half went on has to be *visible* as such.
fn outcome(replaced: u8, rebuilt: bool, was_installed: bool) -> Action {
    Action {
        kind: match (replaced, was_installed) {
            (0, _) => Kind::Failed,
            (_, true) => Kind::Reinstalled,
            (_, false) => Kind::Installed,
        },
        rebuilt,
        replaced,
        failed: (REPLACED.len() as u8).saturating_sub(replaced),
    }
}

/// Makes the ends that still run code put the cursor back: a Rust panic and
/// an unhandled Win32 exception (ADR 0067 decision 3).
///
/// A hard kill runs nothing, by design — that case is covered by the tint
/// only ever meaning "IME is on", so a cursor left behind is legible as
/// "WinRemap died", and by [`restore`] at the next start.
pub fn install_crash_restore() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Silent: see `restore_inner`. The panic message is the diagnosis
        // here, and a log line about the cursor would be competing with it.
        restore_inner(false);
        previous(info);
    }));
    // SAFETY: the filter is a plain `extern "system"` function with no
    // state; passing it is the documented way to be told about an unhandled
    // exception before the process dies.
    unsafe { SetUnhandledExceptionFilter(Some(on_unhandled_exception)) };
}

/// Runs on the failing thread with the process about to die: restore, then
/// let the previous filter (the CRT's, which reports the crash) run.
unsafe extern "system" fn on_unhandled_exception(_info: *const EXCEPTION_POINTERS) -> i32 {
    if INSTALLED.load(Ordering::Relaxed) {
        restore_inner(false);
    }
    // EXCEPTION_CONTINUE_SEARCH: this is a cleanup hook, not a handler.
    0
}
