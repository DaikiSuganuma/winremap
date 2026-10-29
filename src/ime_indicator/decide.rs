//! What one IME sample means for the panel and the cursor, as a pure
//! function of the sample, the settings and what was shown last (v1.1 plan
//! C4). The rules it encodes have each cost an acceptance round: never
//! flash over the shell (ADR 0023), re-flash when the same app is refocused
//! (ADR 0026), and never clear the tint on an unanswered query (ADR 0072).

use super::detect::Sample;
use winremap::ime_indicator_settings::IndicatorSettings;

/// What was observed last time, which is what "became ON" and "a newly
/// focused window" are measured against.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) struct Last {
    pub on: bool,
    pub target: isize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Panel {
    /// Show (or re-show) the panel over the sample's target.
    Show,
    Hide,
    /// Still on, same window: leave whatever is up alone.
    Keep,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Verdict {
    /// Taskbar or desktop: never flash over the shell itself (ADR 0023).
    /// The last target is forgotten so that refocusing the previous app
    /// re-flashes the panel when its IME is still on (ADR 0026).
    ShellSurface,
    Query {
        panel: Panel,
        /// `Some(open)` when the tint is on and the state is *known*: the
        /// cursor follows the state itself, not the "became on" edge the
        /// panel flashes on. An unanswered query leaves it alone (ADR 0072).
        cursor: Option<bool>,
    },
}

/// Decides, and returns what to remember for the next sample.
pub(super) fn decide(sample: &Sample, settings: &IndicatorSettings, last: Last) -> (Verdict, Last) {
    if sample.shell_surface {
        return (Verdict::ShellSurface, Last { target: 0, ..last });
    }
    let is_on = sample.open == Some(true);
    // Only a *known* state moves the cursor. The panel treats "unknown" as
    // "do not show" (design doc §3.2) because a panel that flashes wrongly
    // is worse than one that stays away — but the cursor is not a flash, it
    // is a state, and clearing it asserts "the IME is off", which an
    // unanswered query does not support. A stale tint is corrected by the
    // next query, while a cleared one stays cleared until the user toggles
    // the IME themselves, because nothing re-queries in between (ADR 0021).
    let cursor = settings
        .change_cursor_color
        .then_some(sample.open)
        .flatten();
    let shown = settings.enabled && is_on && (!last.on || sample.target != last.target);
    let panel = if shown {
        Panel::Show
    } else if !is_on {
        // Also covers "unknown" (query failed): prefer not showing over
        // showing wrongly (design doc §3.2).
        Panel::Hide
    } else {
        Panel::Keep
    };
    (
        Verdict::Query { panel, cursor },
        Last {
            on: is_on,
            target: sample.target,
        },
    )
}
