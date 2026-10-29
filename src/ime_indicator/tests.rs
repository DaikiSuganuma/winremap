//! The indicator's decisions, sample by sample (v1.1 plan C4).

use super::decide::{Last, Panel, Verdict, decide};
use super::detect::Sample;
use winremap::ime_indicator_settings::IndicatorSettings;

fn sample(target: isize, open: Option<bool>) -> Sample {
    Sample {
        target,
        open,
        shell_surface: false,
        via_child: false,
    }
}

fn panel_and_tint() -> IndicatorSettings {
    IndicatorSettings {
        enabled: true,
        change_cursor_color: true,
        ..IndicatorSettings::default()
    }
}

fn query(verdict: Verdict) -> (Panel, Option<bool>) {
    match verdict {
        Verdict::Query { panel, cursor } => (panel, cursor),
        Verdict::ShellSurface => panic!("expected a query verdict"),
    }
}

/// Taskbar and desktop clicks change nothing on screen, but forget the
/// target so the previous app re-flashes when refocused (ADR 0023, 0026).
#[test]
fn a_shell_surface_is_ignored_but_forgets_the_target() {
    let shell = Sample {
        shell_surface: true,
        ..sample(0, None)
    };
    let last = Last {
        on: true,
        target: 42,
    };
    let (verdict, next) = decide(&shell, &panel_and_tint(), last);
    assert_eq!(verdict, Verdict::ShellSurface);
    assert_eq!(
        next,
        Last {
            on: true,
            target: 0
        }
    );
}

/// The panel flashes on "became ON" and on "still ON in a newly focused
/// window", and stays put while the same window stays on.
#[test]
fn the_panel_flashes_on_the_edge_and_on_a_new_window() {
    let settings = panel_and_tint();
    let (became_on, last) = decide(&sample(1, Some(true)), &settings, Last::default());
    assert_eq!(query(became_on), (Panel::Show, Some(true)));
    assert_eq!(
        last,
        Last {
            on: true,
            target: 1
        }
    );

    let (still_on, last) = decide(&sample(1, Some(true)), &settings, last);
    assert_eq!(query(still_on).0, Panel::Keep);

    let (new_window, _) = decide(&sample(2, Some(true)), &settings, last);
    assert_eq!(query(new_window).0, Panel::Show);
}

/// OFF hides the panel and clears the tint; an unanswered query hides the
/// panel but leaves the tint alone (ADR 0072).
#[test]
fn off_clears_the_tint_but_unknown_does_not() {
    let settings = panel_and_tint();
    let last = Last {
        on: true,
        target: 1,
    };
    let (off, next) = decide(&sample(1, Some(false)), &settings, last);
    assert_eq!(query(off), (Panel::Hide, Some(false)));
    assert!(!next.on);

    let (unknown, next) = decide(&sample(1, None), &settings, last);
    assert_eq!(query(unknown), (Panel::Hide, None));
    assert!(!next.on, "unknown reads as not-on for the panel's edge");
}

/// The tint works without the panel (ADR 0067; acceptance M-6), and the
/// panel without the tint.
#[test]
fn the_panel_and_the_tint_are_independent() {
    let tint_only = IndicatorSettings {
        enabled: false,
        change_cursor_color: true,
        ..IndicatorSettings::default()
    };
    let (verdict, _) = decide(&sample(1, Some(true)), &tint_only, Last::default());
    // `Keep`, not `Hide`: with the panel switched off nothing was ever shown,
    // and the settings message is what hides a panel left over from before.
    assert_eq!(query(verdict), (Panel::Keep, Some(true)));

    let panel_only = IndicatorSettings {
        enabled: true,
        change_cursor_color: false,
        ..IndicatorSettings::default()
    };
    let (verdict, _) = decide(&sample(1, Some(true)), &panel_only, Last::default());
    assert_eq!(query(verdict), (Panel::Show, None));
}
