//! The DPI context a cursor is read in (ADR 0076).

#[cfg(test)]
use windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_UNAWARE, SetThreadDpiAwarenessContext,
};

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
pub(super) struct Unscaled(DPI_AWARENESS_CONTEXT);

impl Unscaled {
    pub(super) fn enter() -> Self {
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
pub(super) const NOTHING_DRAWN: &str = "the cursor has nothing drawn in it";

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
pub(super) fn unscaled_retry<T, E>(
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

/// Test-only: makes the calling thread per-monitor DPI aware for the length
/// of `f`, which is what `winremap.exe` is and a cargo-test binary is not.
/// Lives here rather than in `tests.rs` so the test file stays `unsafe`-free
/// (ADR 0084 decision 2).
#[cfg(test)]
pub(super) fn as_a_dpi_aware_app<T>(f: impl FnOnce() -> T) -> T {
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
