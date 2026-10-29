//! The `[macro]` section: pacing (ADR 0018/0019/0039) and the recording
//! keys (ADR 0043), plus the check that no keymap rule shadows a recording
//! key.

use super::issue_at_offset;
use crate::config::Issue;
use crate::config::raw::{RawConfig, RawMacro};
use crate::keymap::{
    KeyCombo, Keymap, Layout, MAX_MACRO_DELAY_MS, is_modifier_vk, parse_key_combo,
};
use crate::recorder::RecordKeys;

/// Record keys plus where each was written, so a collision found after the
/// keymaps are compiled can still point at the `[macro]` line.
pub(in crate::config) struct CompiledRecordKeys {
    pub(in crate::config) keys: RecordKeys,
    /// `(label, key, byte offset)` for each configured key. `record_stop`
    /// shares `record_start`'s offset when it was omitted.
    positions: Vec<(&'static str, KeyCombo, usize)>,
}

/// Validates the recording keys in `[macro]` (ADR 0043, design doc §4).
/// Returns `None` when the feature is off — writing none of the three keys
/// is the normal, supported state.
pub(in crate::config) fn compile_macro_record(
    raw: Option<&RawMacro>,
    source: &str,
    layout: &Layout,
    issues: &mut Vec<Issue>,
) -> Option<CompiledRecordKeys> {
    let raw = raw?;
    if raw.record_start.is_none() && raw.record_stop.is_none() && raw.record_play.is_none() {
        return None;
    }

    let mut parse_field = |field: Option<&toml::Spanned<String>>, name: &str| match field {
        None => None,
        Some(spanned) => match parse_key_combo(spanned.get_ref(), layout) {
            // A modifier key never reaches this check in the hook — the
            // callback consumes modifiers for chord state before any lookup —
            // so reject it here instead of leaving a key that never fires.
            Ok(combo) if is_modifier_vk(combo.vk) => {
                issues.push(issue_at_offset(
                    source,
                    spanned.span().start,
                    &format!(
                        "`macro.{name}`: modifier key `{}` cannot be a recording key",
                        spanned.get_ref()
                    ),
                ));
                None
            }
            Ok(combo) => Some((combo, spanned.span().start)),
            Err(e) => {
                issues.push(issue_at_offset(
                    source,
                    spanned.span().start,
                    &format!("`macro.{name}`: {e}"),
                ));
                None
            }
        },
    };

    let start = parse_field(raw.record_start.as_ref(), "record_start");
    let stop = parse_field(raw.record_stop.as_ref(), "record_stop");
    let play = parse_field(raw.record_play.as_ref(), "record_play");

    // Recording needs a way in and a way to use the result; either alone is
    // a half-configured feature, which is more likely a mistake than intent.
    let (Some((start, start_offset)), Some((play, play_offset))) = (start, play) else {
        // Only complain about the missing half when the present half parsed;
        // a parse error above already said what is wrong with this section.
        if issues.is_empty() {
            let offset = raw
                .record_start
                .as_ref()
                .or(raw.record_play.as_ref())
                .or(raw.record_stop.as_ref())
                .map_or(0, |spanned| spanned.span().start);
            issues.push(issue_at_offset(
                source,
                offset,
                "`macro.record_start` and `macro.record_play` must both be set to enable macro recording",
            ));
        }
        return None;
    };

    // Omitted stop means the start key toggles, which is the documented
    // default shape (`S-F10` starts, `S-F10` ends).
    let (stop, stop_offset) = stop.unwrap_or((start, start_offset));

    if play == start || play == stop {
        issues.push(issue_at_offset(
            source,
            play_offset,
            &format!(
                "`macro.record_play` (`{play}`) must differ from the recording keys; one key cannot both end a recording and replay it"
            ),
        ));
        return None;
    }

    Some(CompiledRecordKeys {
        keys: RecordKeys { start, stop, play },
        positions: vec![
            ("record_start", start, start_offset),
            ("record_stop", stop, stop_offset),
            ("record_play", play, play_offset),
        ],
    })
}

/// Rejects a recording key that a keymap also remaps.
///
/// Record keys are intercepted before any keymap lookup and apply to every
/// application, so such a rule can never fire — and nothing in the config
/// file shows that. Unlike keymap-vs-keymap overlaps, which express a real
/// priority (app-specific beats global) and are only surfaced in the
/// settings window, this is silent dead configuration (design doc §4).
pub(in crate::config) fn check_record_key_collisions(
    record: &CompiledRecordKeys,
    keymaps: &[Keymap],
    source: &str,
    issues: &mut Vec<Issue>,
) {
    for (index, &(name, key, offset)) in record.positions.iter().enumerate() {
        // A toggle key is listed twice (start and stop); report it once.
        if record.positions[..index]
            .iter()
            .any(|&(_, earlier, _)| earlier == key)
        {
            continue;
        }
        for keymap in keymaps {
            // Bare rules match on the VK alone, so `F10 = "Home"` shadows
            // every chord on F10, `S-F10` included.
            let shadowed = keymap.exact.contains_key(&key)
                || keymap.seqs.contains_key(&key)
                || keymap.bare.contains_key(&key.vk);
            if shadowed {
                issues.push(issue_at_offset(
                    source,
                    offset,
                    &format!(
                        "`macro.{name}` (`{key}`) is also remapped in `{}`; recording keys are always taken first, so that rule would never fire",
                        keymap.name
                    ),
                ));
            }
        }
    }
}

/// `[macro] delay_ms`, falling back to the v0.1 top-level `macro_delay_ms`
/// (ADR 0039). Setting both is an error rather than a silent precedence: the
/// user would otherwise never learn which one is in effect.
pub(in crate::config) fn compile_macro_delay(
    raw: &RawConfig,
    source: &str,
    issues: &mut Vec<Issue>,
) -> u32 {
    let new = raw.macro_section.as_ref().and_then(|m| m.delay_ms.as_ref());
    let old = raw.macro_delay_ms.as_ref();
    if let (Some(new), Some(old)) = (new, old) {
        issues.push(issue_at_offset(
            source,
            new.span().start.max(old.span().start),
            "both `[macro] delay_ms` and the deprecated top-level `macro_delay_ms` are set; keep only `[macro] delay_ms`",
        ));
    }
    let (spanned, key) = match (new, old) {
        (Some(new), _) => (new, "[macro] delay_ms"),
        (None, Some(old)) => (old, "macro_delay_ms"),
        (None, None) => return 0,
    };
    let value = *spanned.get_ref();
    if value > MAX_MACRO_DELAY_MS {
        issues.push(issue_at_offset(
            source,
            spanned.span().start,
            &format!("`{key}` must be 0-{MAX_MACRO_DELAY_MS} (got {value})"),
        ));
        return 0;
    }
    value
}
