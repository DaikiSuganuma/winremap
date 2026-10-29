//! The `[ime_indicator]` section (ADR 0020, config-spec §6).

use super::issue_at_offset;
use crate::config::Issue;
use crate::config::raw::RawImeIndicator;
use crate::ime_indicator_settings::{
    IndicatorSettings, MAX_INDICATOR_DURATION_MS, MAX_INDICATOR_SIZE, MIN_INDICATOR_DURATION_MS,
    MIN_INDICATOR_SIZE, parse_hex_color,
};
use crate::keymap::{Layout, is_modifier_vk, parse_key_combo};

/// Validates the `[ime_indicator]` section against the ranges in
/// config-spec §6, falling back to the field's default on violation so all
/// issues are still collected in one pass.
pub(in crate::config) fn compile_ime_indicator(
    raw: Option<RawImeIndicator>,
    source: &str,
    layout: &Layout,
    issues: &mut Vec<Issue>,
) -> IndicatorSettings {
    let defaults = IndicatorSettings::default();
    let Some(raw) = raw else {
        return defaults;
    };
    let mut ranged =
        |field: Option<toml::Spanned<u32>>, name: &str, min: u32, max: u32, def| match field {
            Some(value) if !(min..=max).contains(value.get_ref()) => {
                issues.push(issue_at_offset(
                    source,
                    value.span().start,
                    &format!(
                        "`ime_indicator.{name}` must be {min}-{max} (got {})",
                        value.get_ref()
                    ),
                ));
                def
            }
            Some(value) => *value.get_ref(),
            None => def,
        };
    let duration_ms = ranged(
        raw.duration_ms,
        "duration_ms",
        MIN_INDICATOR_DURATION_MS,
        MAX_INDICATOR_DURATION_MS,
        defaults.duration_ms,
    );
    let size = ranged(
        raw.size,
        "size",
        MIN_INDICATOR_SIZE,
        MAX_INDICATOR_SIZE,
        defaults.size,
    );
    let opacity = ranged(raw.opacity, "opacity", 0, 255, defaults.opacity.into());
    let mut trigger_keys = Vec::new();
    for item in raw.trigger_keys.into_iter().flatten() {
        match parse_key_combo(item.get_ref(), layout) {
            // Modifier keys never reach the indicator's key check (the hook
            // consumes them for chord state first), so reject them here
            // instead of silently never firing.
            Ok(combo) if is_modifier_vk(combo.vk) => issues.push(issue_at_offset(
                source,
                item.span().start,
                &format!(
                    "`ime_indicator.trigger_keys`: modifier key `{}` cannot be a trigger",
                    item.get_ref()
                ),
            )),
            Ok(combo) => trigger_keys.push(combo),
            Err(e) => issues.push(issue_at_offset(
                source,
                item.span().start,
                &format!("`ime_indicator.trigger_keys`: {e}"),
            )),
        }
    }
    let cursor_color = match raw.cursor_color {
        None => defaults.cursor_color,
        Some(value) => match parse_hex_color(value.get_ref()) {
            Some(rgb) => rgb,
            None => {
                issues.push(issue_at_offset(
                    source,
                    value.span().start,
                    &format!(
                        "`ime_indicator.cursor_color` must be `#rrggbb` (got `{}`)",
                        value.get_ref()
                    ),
                ));
                defaults.cursor_color
            }
        },
    };
    IndicatorSettings {
        enabled: raw.enabled.unwrap_or(defaults.enabled),
        duration_ms,
        size,
        // Cast is lossless: the 0-255 range was just enforced above.
        opacity: opacity as u8,
        trigger_keys,
        show_app_name: raw.show_app_name.unwrap_or(defaults.show_app_name),
        change_cursor_color: raw
            .change_cursor_color
            .unwrap_or(defaults.change_cursor_color),
        cursor_color,
    }
}
