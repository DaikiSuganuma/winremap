//! The IME indicator and the cursor tint: the detail lines they log and the
//! failures they report (ADR 0020, ADR 0067, ADR 0073).

use super::{Lang, lang};

/// Indicator-thread debug line: one query outcome and what was done.
/// `via_child` marks answers that came from a child window's input thread
/// rather than the foreground window's own (ADR 0033), so detection issues in
/// UWP and WinUI 3 apps are visible in reports.
pub fn debug_ime_query(open: Option<bool>, shown: bool, via_child: bool) -> String {
    match lang() {
        Lang::En => {
            let state = match open {
                Some(true) => "ON",
                Some(false) => "OFF",
                None => "unknown",
            };
            let action = if shown { "panel shown" } else { "no panel" };
            let via = if via_child { " (via child window)" } else { "" };
            format!("state={state} → {action}{via}")
        }
        Lang::Ja => {
            let state = match open {
                Some(true) => "オン",
                Some(false) => "オフ",
                None => "不明",
            };
            let action = if shown {
                "パネル表示"
            } else {
                "表示なし"
            };
            let via = if via_child {
                "（子ウィンドウ経由）"
            } else {
                ""
            };
            format!("状態={state} → {action}{via}")
        }
    }
}

/// What the last [`crate::cursor::apply`] did, for the debug log.
///
/// The distinction that matters is "installed" versus "reinstalled": the
/// caller runs on every foreground change, so an already-installed tint is
/// handed to `SetSystemCursor` again and again. v0.8 acceptance M-2 needs to
/// know which of those happened when the I-beam went invisible.
pub fn debug_cursor_action(action: crate::cursor::Action) -> String {
    use crate::cursor::Kind;
    let rebuilt = action.rebuilt;
    let (replaced, failed) = (action.replaced, action.failed);
    match lang() {
        Lang::En => {
            let what = match action.kind {
                Kind::Idle => return "cursor: nothing to do".into(),
                Kind::Restored => return "cursor: tint removed".into(),
                Kind::Installed => "tint installed",
                Kind::Reinstalled => "tint installed again (was already on)",
                Kind::Failed => "no cursor could be tinted",
            };
            let built = if rebuilt { ", cursors rebuilt" } else { "" };
            let bad = if failed > 0 {
                format!(", {failed} failed")
            } else {
                String::new()
            };
            format!("cursor: {what} — {replaced} replaced{bad}{built}")
        }
        Lang::Ja => {
            let what = match action.kind {
                Kind::Idle => return "カーソル: 何もしない".into(),
                Kind::Restored => return "カーソル: 着色を外した".into(),
                Kind::Installed => "着色した",
                Kind::Reinstalled => "着色し直した（すでに着色済み）",
                Kind::Failed => "1 つも着色できなかった",
            };
            let built = if rebuilt { "・作り直し" } else { "" };
            let bad = if failed > 0 {
                format!("・失敗 {failed}")
            } else {
                String::new()
            };
            format!("カーソル: {what} — 差し替え {replaced}{bad}{built}")
        }
    }
}

/// Which cursor a [`crate::cursor::Trouble`] is about.
fn cursor_noun(id: windows::Win32::UI::WindowsAndMessaging::SYSTEM_CURSOR_ID) -> &'static str {
    use windows::Win32::UI::WindowsAndMessaging::{OCR_IBEAM, OCR_NORMAL};
    match (lang(), id) {
        (Lang::En, OCR_NORMAL) => "the arrow",
        (Lang::En, OCR_IBEAM) => "the I-beam",
        (Lang::En, _) => "a cursor",
        (Lang::Ja, OCR_NORMAL) => "矢印",
        (Lang::Ja, OCR_IBEAM) => "I ビーム",
        (Lang::Ja, _) => "カーソル",
    }
}

/// How many pristine cursors this run is holding, said at startup.
///
/// The positive case is worth a line of its own: what went wrong for two
/// versions was that the restore's first step failed every time and said
/// nothing, so there was no way to ask whether the safety net was armed.
pub fn debug_cursor_snapshot(kept: usize, of: usize) -> String {
    match lang() {
        Lang::En if kept == of => format!("cursor: {kept} pristine copies held for restoring"),
        Lang::En => format!(
            "cursor: only {kept} of {of} pristine copies could be held; \
             the rest fall back to SPI_SETCURSORS"
        ),
        Lang::Ja if kept == of => format!("カーソル: 復元用に素の複製を {kept} 個持ちました"),
        Lang::Ja => format!(
            "カーソル: 素の複製を {of} 個中 {kept} 個しか持てませんでした。\
             残りは SPI_SETCURSORS に頼ります"
        ),
    }
}

/// A cursor failure that used to be swallowed (ADR 0073 decision 5).
///
/// These say what the owner can otherwise only see the consequence of — a
/// cursor that is not coloured, or one that stayed coloured. The `why` is a
/// Win32 call name and stays in English in both languages, like every other
/// diagnostic.
pub fn debug_cursor_trouble(trouble: crate::cursor::Trouble) -> String {
    use crate::cursor::Trouble;
    let en = matches!(lang(), Lang::En);
    match trouble {
        Trouble::NoSnapshot { id, why } => {
            let what = cursor_noun(id);
            if en {
                format!(
                    "cursor: no pristine copy of {what} could be taken ({why}); \
                     restoring falls back to SPI_SETCURSORS alone"
                )
            } else {
                format!(
                    "カーソル: {what}の複製を採れませんでした（{why}）。\
                     復元は SPI_SETCURSORS だけで行います"
                )
            }
        }
        Trouble::RestoreFailed { id, why } => {
            let what = cursor_noun(id);
            if en {
                format!("cursor: {what} could not be put back ({why})")
            } else {
                format!("カーソル: {what}を戻せませんでした（{why}）")
            }
        }
        Trouble::ReloadFailed => {
            if en {
                "cursor: SPI_SETCURSORS failed; a tint may be left in the session".into()
            } else {
                "カーソル: SPI_SETCURSORS が失敗しました。着色が残っているかもしれません".into()
            }
        }
        Trouble::SourceEmpty { id } => {
            let what = cursor_noun(id);
            if en {
                format!(
                    "cursor: {what} has nothing drawn in it, so it was not tinted \
                     (tinting an empty cursor produces another empty one)"
                )
            } else {
                format!(
                    "カーソル: {what}に描かれている画素がないので、着色しませんでした\
                     （空のカーソルを元にすると、また空ができます）"
                )
            }
        }
        Trouble::ResultEmpty { id } => {
            let what = cursor_noun(id);
            if en {
                format!("cursor: the tint built for {what} was empty and was thrown away")
            } else {
                format!("カーソル: {what}の着色が空になったので、入れずに捨てました")
            }
        }
        Trouble::BuildFailed { id, why } => {
            let what = cursor_noun(id);
            if en {
                format!("cursor: the tint for {what} could not be built ({why})")
            } else {
                format!("カーソル: {what}の着色を作れませんでした（{why}）")
            }
        }
    }
}

/// The IME indicator could not start (or died); remapping keeps running.
/// `error` stays in English on purpose (diagnostics policy above).
pub fn ime_indicator_failed(error: &str) -> String {
    match lang() {
        Lang::En => format!("IME indicator unavailable (remapping is unaffected): {error}"),
        Lang::Ja => {
            format!("IME インジケーターを利用できません（リマップ動作には影響ありません）: {error}")
        }
    }
}
