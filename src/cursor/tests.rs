//! Tests for the cursor tint (moved out of the module file, guidelines §5).
//! Only the arithmetic, and one read (ADR 0073 decision 6 — everything that
//! *replaces* a cursor does it for the whole session, and that is what the
//! acceptance probe is for). The read is here because the thing it catches is
//! invisible to the probe: [`Unscaled`] is about the DPI context of the
//! process asking, and the probe asks from PowerShell.

use super::pixels::outlined;
use super::scale::{Unscaled, as_a_dpi_aware_app};
use super::tint::{drawn_pixels, load, snapshot, tinted};
use super::*;

/// Whether this machine has an I-beam to measure at all.
///
/// The session's cursor table can hold an **empty** I-beam: a run killed
/// while tinted leaves its replacement behind, and any WinRemap before
/// 1.0.0 on a scaled display registered an empty one on purpose (ADR
/// 0076). No thread of any DPI context can read a cursor out of that
/// state — `SPI_SETCURSORS` is what repairs it, and replacing session
/// state belongs to the acceptance probe, not to a unit test (ADR 0073
/// decision 6). Measured 2026-08-16 with 0.9.0 running: the registered
/// I-beam was a 32x32 colour cursor with 0 drawn pixels, and both tests
/// below failed with nothing wrong in the code.
///
/// This is only ever true when the **unscaled** read is empty too, so the
/// failure the two tests exist for — aware reads empty, unaware reads
/// fine — is not hidden by it.
fn the_session_has_no_i_beam() -> bool {
    let _unscaled = Unscaled::enter();
    let drawn = load(IDC_IBEAM).ok().and_then(drawn_pixels);
    if !matches!(drawn, Some(px) if px > 0) {
        // Visible under `--nocapture`, because a test that passes by
        // measuring nothing has to be able to say so.
        eprintln!(
            "skipped: this session has no I-beam to read ({drawn:?} drawn); \
             signing out and back in loads the machine's own cursors afresh"
        );
        return true;
    }
    false
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
    if the_session_has_no_i_beam() {
        return;
    }
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
    if the_session_has_no_i_beam() {
        return;
    }
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
