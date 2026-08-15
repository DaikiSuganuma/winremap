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

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDIBits, GetObjectW, HBITMAP,
    HGDIOBJ,
};
use windows::Win32::System::Diagnostics::Debug::{EXCEPTION_POINTERS, SetUnhandledExceptionFilter};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_UNAWARE, SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CopyIcon, CreateIconIndirect, GetIconInfo, HCURSOR, HICON, ICONINFO, IDC_ARROW, IDC_IBEAM,
    IMAGE_CURSOR, LR_DEFAULTSIZE, LR_SHARED, LoadImageW, OCR_IBEAM, OCR_NORMAL, SPI_SETCURSORS,
    SPIF_SENDCHANGE, SYSTEM_CURSOR_ID, SetSystemCursor, SystemParametersInfoW,
};
use windows::core::PCWSTR;

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

/// Makes the calling thread ask about cursors the way a process that knows
/// nothing about display scaling does, and puts the previous context back on
/// the way out.
///
/// **This is what makes the I-beam readable at all on a scaled display**
/// (ADR 0076). WinRemap is per-monitor DPI aware, and at 150% the system
/// cursor size is 48 rather than 32. Asked from a DPI-aware thread, Windows
/// does not hand out the machine's I-beam at that size — it hands out a
/// 48×48 **colour** cursor whose alpha is zero everywhere and whose AND mask
/// marks nothing opaque. There is nothing in it to tint. Measured on Windows
/// 11 26200 at 150%, 2026-08-15:
///
/// | thread | I-beam | arrow |
/// |---|---|---|
/// | DPI-aware | colour 48×48, **0 drawn** | colour 48×48, 280 drawn |
/// | DPI-unaware | mask-only 32×32, **26 drawn** | colour 32×32, 144 drawn |
///
/// The stock I-beam is the mask-only, invert-drawn cursor that
/// [`from_mask_only`] describes, and that shape does not survive being turned
/// into a 32-bit colour bitmap: an AND=1/XOR=1 pixel has no colour to carry.
/// The arrow is a real colour cursor with alpha, so it scales and is why only
/// half the tint ever went missing.
///
/// The size arguments are no help — `LR_SHARED` hands back the object cached
/// for the thread's DPI context, and `0,0`, `32,32` and `48,48` all returned
/// the same empty 48×48 one (measured the same day). The context is the only
/// lever there is.
struct Unscaled(DPI_AWARENESS_CONTEXT);

impl Unscaled {
    fn enter() -> Self {
        // SAFETY: the call takes one of the documented DPI_AWARENESS_CONTEXT
        // values and returns the thread's previous one, which is an opaque
        // handle owned by the system and only handed back to the same call.
        Self(unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE) })
    }
}

impl Drop for Unscaled {
    fn drop(&mut self) {
        // A null context means the previous one could not be read, and there
        // is then nothing to put back — restoring a null would be the call
        // failing rather than the thread returning to where it was.
        if !self.0.0.is_null() {
            // SAFETY: `self.0` came from the call in `enter` and is the
            // context this thread had before.
            unsafe { SetThreadDpiAwarenessContext(self.0) };
        }
    }
}

/// An empty cursor, told apart from a Win32 call going wrong because it is
/// the one failure worth retrying unscaled. [`snapshot`] reports it as it is,
/// [`tinted`] turns it into [`Trouble::SourceEmpty`].
const NOTHING_DRAWN: &str = "the cursor has nothing drawn in it";

/// **The whole job is retried, not just the read.** A shared cursor handle is
/// resolved against the DPI context of the thread *using* it, not the one
/// that loaded it: a handle fetched unscaled and then decoded by an aware
/// thread comes back empty again. Measured 2026-08-15 — the first version of
/// this fix held [`Unscaled`] across `LoadImageW` alone and changed nothing.
///
/// So the unit of work is "read it and make the thing", and that is what runs
/// again. Retrying rather than always going unscaled keeps the arrow at the
/// size this display uses — 48×48 and native at 150%, instead of a 32×32 copy
/// scaled back up — and needs no list of which cursors are mask-only, a list
/// that would be wrong the moment Windows changes one.
fn unscaled_retry<T, E>(
    was_empty: impl Fn(&E) -> bool,
    mut build: impl FnMut() -> Result<T, E>,
) -> Result<T, E> {
    let first = build();
    match &first {
        Err(why) if was_empty(why) => {}
        // Anything else — a working cursor, or a call that failed for its own
        // reasons — is the answer. Only emptiness is a question of scale.
        _ => return first,
    }
    let _unscaled = Unscaled::enter();
    build()
}

/// Asks the system for one of its own cursors.
///
/// No module handle: that is the spelling that works (measured — the
/// user32-relative one is the `1814` above). LR_SHARED hands back the
/// system's own handle, which must not be destroyed, and is also required for
/// a system cursor — without it the call fails. LR_DEFAULTSIZE gives the size
/// this display is using, which is the part [`unscaled_retry`] second-guesses.
fn load(name: PCWSTR) -> Result<HICON, &'static str> {
    // SAFETY: `name` is one of the IDC_* atoms; a null instance means the
    // predefined cursors.
    let raw: HANDLE =
        unsafe { LoadImageW(None, name, IMAGE_CURSOR, 0, 0, LR_SHARED | LR_DEFAULTSIZE) }
            .map_err(|_| "LoadImageW")?;
    Ok(HICON(raw.0))
}

/// One pristine copy, or why there is not one.
fn snapshot(name: PCWSTR) -> Result<HICON, &'static str> {
    unscaled_retry(|why| *why == NOTHING_DRAWN, || snapshot_at_this_scale(name))
}

fn snapshot_at_this_scale(name: PCWSTR) -> Result<HICON, &'static str> {
    let raw = load(name)?;
    match drawn_pixels(raw) {
        None => return Err("the cursor could not be read"),
        Some(0) => return Err(NOTHING_DRAWN),
        Some(_) => {}
    }
    // SAFETY: a live cursor handle; the copy is ours to keep for the life of
    // the process, and the shared original is left alone.
    unsafe { CopyIcon(raw) }.map_err(|_| "CopyIcon")
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

/// One of the system's cursors read out as pixels this module can work on.
struct Decoded {
    width: i32,
    height: i32,
    /// Top-down BGRA, already recoloured. **A zero alpha means nothing is
    /// drawn there** — which is what makes counting emptiness the same job
    /// for both kinds of source.
    pixels: Vec<u32>,
    hotspot: (u32, u32),
}

/// Reads a cursor and recolours it, whichever of the two shapes it has:
/// modern cursors carry a 32-bit colour bitmap with alpha, while the classic
/// I-beam is a mask-only cursor whose bitmap is twice as tall (AND rows over
/// XOR rows). Reading either through `GetDIBits` as 32-bit makes them the
/// same problem.
fn decode(icon: HICON, color: (u8, u8, u8)) -> Option<Decoded> {
    let mut info = ICONINFO::default();
    // SAFETY: `icon` is a live cursor handle; `info` is a live local.
    unsafe { GetIconInfo(icon, &mut info) }.ok()?;
    // GetIconInfo hands out bitmap copies that the caller owns.
    let mask = Bitmap(info.hbmMask);
    let color_bmp = Bitmap(info.hbmColor);

    let (width, height, pixels) = if color_bmp.0.is_invalid() {
        from_mask_only(mask.0, color)?
    } else {
        from_color(color_bmp.0, mask.0, color)?
    };
    Some(Decoded {
        width,
        height,
        pixels,
        hotspot: (info.xHotspot, info.yHotspot),
    })
}

/// How many pixels of a cursor would actually be drawn — `None` if it could
/// not be read at all.
///
/// **Zero is the whole of M-2 in one number.** What the owner saw four times
/// was a cursor Windows was perfectly happy to install and draw nothing of.
/// Nothing measured it, on either side: not the tint on its way in, not the
/// cursor it was made from (ADR 0073 decisions 3 and 4).
fn drawn_pixels(icon: HICON) -> Option<usize> {
    // White because it has to be something; the count is of alpha, which
    // recolouring never touches.
    let decoded = decode(icon, (0xFF, 0xFF, 0xFF))?;
    Some(decoded.pixels.iter().filter(|px| *px >> 24 != 0).count())
}

/// Builds a recoloured copy of one of the system's cursors, or says why it
/// could not (ADR 0073 decision 5 — this used to be five silent `?`s).
fn tinted(id: SYSTEM_CURSOR_ID, name: PCWSTR, color: (u8, u8, u8)) -> Result<HCURSOR, Trouble> {
    // An empty source is retried unscaled (ADR 0076); anything else it says
    // is final. `SourceEmpty` therefore now means what it says — the cursor
    // is empty however it is asked for, not merely at this display's scale.
    unscaled_retry(
        |trouble| matches!(trouble, Trouble::SourceEmpty { .. }),
        || tinted_at_this_scale(id, name, color),
    )
}

fn tinted_at_this_scale(
    id: SYSTEM_CURSOR_ID,
    name: PCWSTR,
    color: (u8, u8, u8),
) -> Result<HCURSOR, Trouble> {
    let raw = match load(name) {
        Ok(raw) => raw,
        Err(why) => return Err(build_failed(id, why)),
    };
    let Some(decoded) = decode(raw, color) else {
        return Err(build_failed(id, "the cursor could not be read"));
    };
    let Decoded {
        width,
        height,
        pixels,
        hotspot,
    } = decoded;
    // Decision 4: an empty source produces an empty tint, and `from_color`
    // falls back to the AND mask when it finds no alpha — so an empty one
    // feeds itself. This is also what `unscaled_retry` watches for, so on a
    // scaled display it is the trigger for reading the cursor again unscaled
    // rather than the end of the road (ADR 0076).
    if pixels.iter().all(|px| px >> 24 == 0) {
        return Err(Trouble::SourceEmpty { id });
    }

    let pixels = outlined(width, height, &pixels);
    // Decision 3: and this is where an empty *result* stops, whatever made
    // it. `outlined` can produce one from a source that is not empty — a
    // cursor whose every pixel is less than half opaque has no solid shape
    // to keep and no solid neighbour to draw a border around.
    if pixels.iter().all(|px| px >> 24 == 0) {
        return Err(Trouble::ResultEmpty { id });
    }
    let bits = and_mask(width, height, &pixels);
    // SAFETY: a 1bpp monochrome bitmap; `bits` is sized to its stride (rows
    // padded to a 16-bit boundary) times its height.
    let shape = Bitmap(unsafe {
        CreateBitmap(
            width,
            height,
            1,
            1,
            Some(bits.as_ptr() as *const core::ffi::c_void),
        )
    });
    let Some(section) = dib_section(width, height, &pixels) else {
        return Err(build_failed(id, "CreateDIBSection"));
    };
    let colored = Bitmap(section);
    let icon_info = ICONINFO {
        fIcon: false.into(),
        xHotspot: hotspot.0,
        yHotspot: hotspot.1,
        hbmMask: shape.0,
        hbmColor: colored.0,
    };
    // SAFETY: both bitmaps are live and of the same size; fIcon = false asks
    // for a cursor, which is what the hotspots are for.
    match unsafe { CreateIconIndirect(&icon_info) } {
        Ok(icon) => Ok(HCURSOR(icon.0)),
        Err(_) => Err(build_failed(id, "CreateIconIndirect")),
    }
}

/// Names the Win32 call that would not do its part.
///
/// Only the failures with a call to name go through here; an empty source or
/// an empty result are their own [`Trouble`] and say so themselves.
fn build_failed(id: SYSTEM_CURSOR_ID, why: &'static str) -> Trouble {
    Trouble::BuildFailed { id, why }
}

/// Puts a white border around the cursor's solid shape, and drops whatever
/// was outside it.
///
/// **Without this the tint is invisible on a dark application** (owner
/// report, 2026-08-02). The reason is arithmetic: the tint maps the cursor's
/// white body to the chosen colour, and `#0078d4` is about 37% as bright as
/// white. What used to be the brightest thing on the screen becomes a middling
/// one — fine on a white page, lost on a black editor.
///
/// A border in the opposite direction fixes it by construction: the cursor is
/// then made of two colours at opposite ends of the brightness range, so
/// **whatever the background, one of them stands out**. Windows takes the same
/// approach for its own custom-coloured pointer.
///
/// Synthesised rather than recoloured, because there is often nothing to
/// recolour. The stock arrow on Windows 11 has **no black outline at all** —
/// measured 2026-08-02: 77 opaque white pixels and 67 semi-transparent ones
/// making a soft drop shadow, and not one opaque black pixel. The I-beam has
/// no outline either. So the border is drawn from the shape itself: solid
/// pixels keep the tint, their transparent neighbours become opaque white,
/// and the rest — the old shadow, which a white border makes redundant —
/// goes away.
fn outlined(width: i32, height: i32, pixels: &[u32]) -> Vec<u32> {
    let (w, h) = (width as usize, height as usize);
    // Half-transparent is the dividing line: the anti-aliased fringe of the
    // body belongs to the border, not to the shape.
    let solid: Vec<bool> = pixels.iter().map(|px| px >> 24 >= 128).collect();
    let mut out = vec![0u32; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if solid[i] {
                out[i] = pixels[i];
                continue;
            }
            let touches = (y.saturating_sub(1)..=(y + 1).min(h - 1))
                .any(|ny| (x.saturating_sub(1)..=(x + 1).min(w - 1)).any(|nx| solid[ny * w + nx]));
            if touches {
                out[i] = 0xFFFF_FFFF;
            }
        }
    }
    out
}

/// The 1-bit AND mask that says where the screen shows through: 1 for the
/// pixels whose alpha is zero, 0 for the rest.
///
/// **Not redundant with the alpha channel — it is the fallback for whoever
/// ignores it.** Windows has more than one path for putting a cursor on the
/// screen, and a 32-bit colour bitmap with an all-zero mask is only right on
/// the paths that read alpha; the others take the mask at its word and draw a
/// **solid rectangle**. Zed showed exactly that (owner report, 2026-08-02),
/// and the earlier "the alpha was uniformly zero" bug was the same failure
/// from the other end. Filling the mask in means the shape survives either
/// way — the alpha path additionally keeps the anti-aliased edge.
///
/// Rows are padded to a 16-bit boundary and the leftmost pixel is the high
/// bit of the first byte, which is what `CreateBitmap` expects of a
/// monochrome bitmap.
fn and_mask(width: i32, height: i32, pixels: &[u32]) -> Vec<u8> {
    let stride = (((width + 15) / 16) * 2) as usize;
    let mut bits = vec![0u8; stride * height as usize];
    for y in 0..height as usize {
        for x in 0..width as usize {
            if pixels[y * width as usize + x] >> 24 == 0 {
                bits[y * stride + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    bits
}

/// Copies 32-bit BGRA pixels into a bitmap a cursor can be built from.
///
/// A DIB section, not `CreateBitmap`: `CreateIconIndirect` only honours the
/// alpha channel of a **device-independent** 32-bit bitmap. Given a
/// device-dependent one it ignores alpha and falls back to the AND mask,
/// which here says "opaque everywhere" — and the cursor comes out as a solid
/// rectangle. That is what an I-beam looked like on the first attempt: a
/// black square (owner report, 2026-08-02).
fn dib_section(width: i32, height: i32, pixels: &[u32]) -> Option<HBITMAP> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative: top-down, matching the order `pixels` is in.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: `info` describes the buffer the call hands back through `bits`;
    // a null DC is allowed for DIB_RGB_COLORS.
    let bitmap =
        unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()?;
    if bits.is_null() {
        // SAFETY: the bitmap was just created here and is not used again.
        let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
        return None;
    }
    // SAFETY: the section is width*height 32-bit pixels, exactly the length
    // of `pixels`, and nothing else refers to it yet.
    unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u32, pixels.len()) };
    Some(bitmap)
}

/// A modern cursor: colour bitmap plus, for the ones without alpha, the AND
/// mask that says which pixels are transparent.
fn from_color(
    color_bmp: HBITMAP,
    mask: HBITMAP,
    color: (u8, u8, u8),
) -> Option<(i32, i32, Vec<u32>)> {
    let (width, height, source) = read_as_bgra(color_bmp)?;
    let has_alpha = source.iter().any(|px| px >> 24 != 0);
    let and_mask = if has_alpha {
        None
    } else {
        read_as_bgra(mask).map(|(_, _, bits)| bits)
    };
    let pixels = source
        .iter()
        .enumerate()
        .map(|(i, px)| {
            let opaque = match &and_mask {
                // The AND mask is 1 where the background shows through.
                Some(bits) => bits.get(i).is_some_and(|m| m & 0x00FF_FFFF == 0),
                None => px >> 24 != 0,
            };
            if !opaque {
                return 0;
            }
            let alpha = if has_alpha { px >> 24 } else { 0xFF };
            tint(*px, color) | (alpha << 24)
        })
        .collect();
    Some((width, height, pixels))
}

/// A classic mask-only cursor (the default I-beam is one): the bitmap is
/// twice as tall, the AND rows above the XOR rows. The two bits together pick
/// one of four things to do with the pixel underneath:
///
/// | AND | XOR | classic meaning | here |
/// |---|---|---|---|
/// | 1 | 0 | leave the screen alone | transparent |
/// | 1 | 1 | **invert** the screen | the colour |
/// | 0 | 0 | black | black |
/// | 0 | 1 | white | the colour |
///
/// The invert row is not a corner case: **the stock I-beam is drawn entirely
/// out of it** (measured 2026-08-02 — its AND plane is 1 everywhere, and all
/// 26 visible pixels are AND=1/XOR=1). That is how it stays legible on a dark
/// background as well as a light one. Dropping those pixels as "no colour to
/// tint" left a cursor with nothing opaque in it at all, and Windows reads a
/// 32-bit bitmap whose alpha is uniformly zero as having no alpha channel: it
/// falls back to the AND mask, which here is opaque everywhere, and draws the
/// all-black colour bitmap as a **solid black square** (owner report,
/// 2026-08-02).
///
/// Painting them the tint colour loses the inversion — on a background of
/// that same colour the I-beam is harder to pick out than the stock one. That
/// is the trade this feature already makes everywhere else: the point is that
/// the colour says the IME is on.
fn from_mask_only(mask: HBITMAP, color: (u8, u8, u8)) -> Option<(i32, i32, Vec<u32>)> {
    let (width, double_height, bits) = read_as_bgra(mask)?;
    let height = double_height / 2;
    if height == 0 {
        return None;
    }
    let plane = (width * height) as usize;
    let pixels = (0..plane)
        .map(|i| {
            let and = bits[i] & 0x00FF_FFFF != 0;
            let lit = bits[plane + i] & 0x00FF_FFFF != 0;
            match (and, lit) {
                (true, false) => 0,
                (_, true) => 0xFF00_0000 | tint(0x00FF_FFFF, color),
                (false, false) => 0xFF00_0000,
            }
        })
        .collect();
    Some((width, height, pixels))
}

/// Recolours one pixel: its brightness decides how much of `color` it gets.
/// Black outlines stay black, a white body becomes the colour, and the
/// anti-aliased edge in between keeps its shading.
fn tint(bgra: u32, (r, g, b): (u8, u8, u8)) -> u32 {
    let (sb, sg, sr) = (bgra & 0xFF, (bgra >> 8) & 0xFF, (bgra >> 16) & 0xFF);
    // Rec. 601 luma, the same weights the eye applies.
    let luma = (sr * 30 + sg * 59 + sb * 11) / 100;
    let scale = |c: u8| ((u32::from(c) * luma) / 255) & 0xFF;
    (scale(r) << 16) | (scale(g) << 8) | scale(b)
}

/// Reads any bitmap as top-down 32-bit BGRA, whatever it is stored as.
/// Monochrome sources come back as black and white, which is what makes the
/// mask-only path above simple.
fn read_as_bgra(bitmap: HBITMAP) -> Option<(i32, i32, Vec<u32>)> {
    let mut header = BITMAP::default();
    // SAFETY: `header` is a live local of exactly the size passed.
    let read = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0),
            size_of::<BITMAP>() as i32,
            Some(&mut header as *mut BITMAP as *mut core::ffi::c_void),
        )
    };
    if read == 0 || header.bmWidth <= 0 || header.bmHeight <= 0 {
        return None;
    }
    let (width, height) = (header.bmWidth, header.bmHeight);
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative: top-down, so row 0 is the top one and the mask-only
            // path can say "the AND rows come first".
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u32; (width * height) as usize];
    // SAFETY: a null DC is allowed for GetDIBits; the buffer is sized from
    // the header the call is given.
    let dc = unsafe { CreateCompatibleDC(None) };
    let lines = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut core::ffi::c_void),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    // SAFETY: `dc` was just created here and is not used again.
    let _ = unsafe { DeleteDC(dc) };
    (lines != 0).then_some((width, height, pixels))
}

/// Deletes a GDI bitmap on the way out. GetIconIndirect and GetIconInfo both
/// hand out bitmaps the caller has to free, and this feature builds four of
/// them per cursor.
struct Bitmap(HBITMAP);

impl Drop for Bitmap {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: the handle is ours and is not referenced afterwards —
            // CreateIconIndirect copies what it is given.
            let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
        }
    }
}

/// Only the arithmetic, and one read (ADR 0073 decision 6 — everything that
/// *replaces* a cursor does it for the whole session, and that is what the
/// acceptance probe is for). The read is here because the thing it catches is
/// invisible to the probe: [`Unscaled`] is about the DPI context of the
/// process asking, and the probe asks from PowerShell.
#[cfg(test)]
mod tests {
    use windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;

    use super::*;

    /// Runs `f` on a thread claiming to be per-monitor DPI aware — what
    /// `winremap.exe` is, and what a cargo-test binary is not.
    fn as_a_dpi_aware_app<T>(f: impl FnOnce() -> T) -> T {
        // SAFETY: a documented DPI_AWARENESS_CONTEXT value; the return is the
        // thread's previous context, put back below.
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let out = f();
        if !previous.0.is_null() {
            // SAFETY: `previous` came from the call above.
            unsafe { SetThreadDpiAwarenessContext(previous) };
        }
        out
    }

    /// **The failure the owner reported on 2026-08-15, as an assertion**
    /// (ADR 0076): with the IME on, the arrow was tinted and the I-beam was
    /// not, because `winremap.exe` is per-monitor DPI aware and a DPI-aware
    /// thread at 150% is handed an I-beam with nothing drawn in it.
    ///
    /// Builds only — nothing is installed, so this stays inside the rule that
    /// replacing a cursor belongs to the acceptance probe.
    ///
    /// **It only has teeth on a scaled display.** At 100% there is no scaled
    /// form to be handed, so it passes with or without the fix — which is
    /// exactly how the bug shipped: everything that measured this feature (CI,
    /// the probe in PowerShell, a cargo-test binary with no manifest) was
    /// effectively unscaled. Kept because the machine it is developed on is at
    /// 150%, and that is where it fails first.
    #[test]
    fn a_dpi_aware_app_can_still_tint_the_i_beam() {
        let built = as_a_dpi_aware_app(|| tinted(OCR_IBEAM, IDC_IBEAM, (0x00, 0x78, 0xD4)));
        let drawn = built.map(|cursor| drawn_pixels(HICON(cursor.0)));
        assert!(
            matches!(drawn, Ok(Some(px)) if px > 0),
            "the I-beam tint must survive this display's scale, got {drawn:?}"
        );
    }

    /// And the safety net over the same ground: without a pristine copy of
    /// the I-beam there is nothing for [`restore`] to put back, which is how
    /// the scaled read disabled ADR 0073's first step as well as the tint.
    #[test]
    fn a_dpi_aware_app_can_still_snapshot_the_i_beam() {
        let taken = as_a_dpi_aware_app(|| snapshot(IDC_IBEAM));
        assert!(
            taken.is_ok_and(|icon| matches!(drawn_pixels(icon), Some(px) if px > 0)),
            "a pristine copy has to have the cursor in it"
        );
    }

    /// **The empty tint of M-2 can be built from a source that is not
    /// itself empty**, which is why decision 3 checks the result rather than
    /// trusting the source check of decision 4. `outlined` keeps only pixels
    /// that are at least half opaque and draws the border from those, so a
    /// cursor made entirely of the anti-aliased fringe has neither.
    #[test]
    fn a_source_of_nothing_but_fringe_outlines_to_nothing() {
        // 0x7F: one step below the half-opaque line `outlined` draws.
        let fringe = vec![0x7F00_00FFu32; 16];
        let out = outlined(4, 4, &fringe);
        assert!(
            out.iter().all(|px| px >> 24 == 0),
            "every pixel should have been dropped: {out:08x?}"
        );
    }

    /// And the ordinary case still comes out with something in it — without
    /// this, the check above would be satisfied by an `outlined` that always
    /// returned nothing.
    #[test]
    fn a_solid_pixel_keeps_its_colour_and_gains_a_white_border() {
        let mut pixels = vec![0u32; 9];
        pixels[4] = 0xFF12_3456;
        let out = outlined(3, 3, &pixels);
        assert_eq!(out[4], 0xFF12_3456, "the solid pixel kept its tint");
        assert!(
            out.iter()
                .enumerate()
                .all(|(i, px)| i == 4 || *px == 0xFFFF_FFFF),
            "all eight neighbours should be the white border: {out:08x?}"
        );
    }

    /// **This is the v0.9 acceptance failure, in one assertion.** The I-beam's
    /// tint could not be built, so it never reached the install loop, so
    /// nothing counted it: the log said the tint went on and named no
    /// failure, while half the cursors on screen were plain. Whether a cursor
    /// goes missing while being built, copied or installed is not something
    /// the count is allowed to care about.
    #[test]
    fn a_tint_that_only_half_went_on_is_counted_as_a_failure() {
        let half = outcome(1, false, false);
        assert_eq!(half.replaced, 1);
        assert_eq!(half.failed, 1, "the cursor that did not go on is missing");
        assert_eq!(half.kind, Kind::Installed, "what did go on, went on");
    }

    /// The other side of it: a whole tint must not report a phantom failure,
    /// or the line above stops meaning anything.
    #[test]
    fn a_whole_tint_reports_nothing_failed() {
        let whole = outcome(REPLACED.len() as u8, true, false);
        assert_eq!(whole.failed, 0);
        assert!(whole.rebuilt);
    }

    /// And none at all is still `Failed` — the case ADR 0073 decision 3 added
    /// so that installing nothing could not read as installing something.
    #[test]
    fn no_cursor_at_all_is_a_failure_of_every_cursor() {
        let none = outcome(0, false, true);
        assert_eq!(none.kind, Kind::Failed);
        assert_eq!(none.failed, REPLACED.len() as u8);
    }
}
