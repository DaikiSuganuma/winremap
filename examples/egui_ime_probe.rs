//! Phase B prototype: does Japanese IME input work in egui text fields?
//!
//! ADR 0030 listed three risks to settle before designing the config GUI.
//! Two are already answered by the tray log window (`src/log_window.rs`):
//! Japanese glyphs render from a system font, and the hook keeps running
//! while an eframe window is up. The third — IME input — needed a text field,
//! which the log window has none of, hence this probe.
//!
//! Run it with `cargo run --example egui_ime_probe`, turn the IME on, and type
//! Japanese into each field. What to check is listed in the window itself and
//! in docs/v0.2/03_acceptance-checklist.md (B-1..B-7).
//!
//! Standalone on purpose: it must be runnable while WinRemap itself is
//! resident, so it installs no hook, no tray icon, and no single-instance
//! guard. It duplicates the font loading from src/log_window.rs rather than
//! sharing it, so the probe keeps working however that file evolves.

use eframe::egui;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("WinRemap — egui IME probe")
        .with_inner_size([720.0, 620.0]);
    if let Ok(icon) =
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/png/kbd-enabled-48.png"))
    {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    eframe::run_native(
        "winremap-egui-ime-probe",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(|cc| {
            install_fonts(&cc.egui_ctx);
            Ok(Box::<Probe>::default())
        }),
    )
}

/// Same candidates and ordering as src/log_window.rs: borrow a CJK face from
/// the system instead of embedding megabytes of font.
fn install_fonts(ctx: &egui::Context) {
    const CANDIDATES: &[(&str, u32)] = &[
        (r"C:\Windows\Fonts\meiryo.ttc", 0),
        (r"C:\Windows\Fonts\YuGothM.ttc", 0),
        (r"C:\Windows\Fonts\YuGothR.ttc", 0),
        (r"C:\Windows\Fonts\msgothic.ttc", 0),
    ];
    let Some((bytes, index)) = CANDIDATES
        .iter()
        .find_map(|&(path, index)| std::fs::read(path).ok().map(|bytes| (bytes, index)))
    else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    let mut data = egui::FontData::from_owned(bytes);
    data.index = index;
    fonts.font_data.insert("system_jp".to_owned(), data.into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("system_jp".to_owned());
    }
    ctx.set_fonts(fonts);
}

#[derive(Default)]
struct Probe {
    /// Stands in for an `application` field: short, single line.
    single: String,
    /// Stands in for a TOML editing pane: multi-line, monospace.
    multi: String,
    /// Stands in for a keymap `name`, to check editing in a table row.
    rows: Vec<String>,
    /// Last IME event seen, so composition can be observed without a debugger.
    ime_events: Vec<String>,
    /// `Visuals::ime_composition.legacy_visuals`, exposed so the two
    /// renderings can be compared side by side. egui defaults it to `true` on
    /// Windows, which draws the preedit as a plain selection — no underline,
    /// and no visible boundary for the segment being converted.
    legacy_ime_visuals: bool,
}

impl eframe::App for Probe {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.rows.is_empty() {
            self.rows = vec!["notepad".to_owned(), "既定".to_owned()];
        }

        // Composition events are what actually distinguishes "the IME works"
        // from "characters happen to arrive": egui surfaces them as
        // Event::Ime, and seeing Preedit → Commit is the proof.
        let events: Vec<String> = ui.ctx().input(|i| {
            i.events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Ime(ime) => Some(format!("{ime:?}")),
                    _ => None,
                })
                .collect()
        });
        for event in events {
            self.ime_events.push(event);
        }
        while self.ime_events.len() > 12 {
            self.ime_events.remove(0);
        }

        egui::CentralPanel::default().show(ui, |ui| {
            // Child widgets inherit this Ui's visuals, so setting it here
            // covers every text field below.
            ui.visuals_mut().ime_composition.legacy_visuals = self.legacy_ime_visuals;

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("egui 日本語 IME プローブ");
                ui.label(
                    "IME をオンにして各欄に日本語を入力し、下のチェック項目を確認してください。",
                );
                ui.checkbox(
                    &mut self.legacy_ime_visuals,
                    "旧来の未確定表示（legacy_visuals: egui の Windows 既定値）",
                );
                ui.label(
                    "オフのときだけ未確定文字に下線が付き、変換中の文節が区別されます。\
                     オン・オフを切り替えて見比べてください。",
                );
                ui.separator();

                ui.label("1. 単一行（設定 GUI の application 欄に相当）");
                ui.text_edit_singleline(&mut self.single);
                ui.add_space(8.0);

                ui.label("2. 複数行・等幅（TOML 編集ペインに相当）");
                ui.add(
                    egui::TextEdit::multiline(&mut self.multi)
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);

                ui.label("3. 表の行内編集（キーマップ一覧に相当）");
                for (index, row) in self.rows.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("keymap[{index}].name"));
                        ui.text_edit_singleline(row);
                    });
                }
                ui.add_space(8.0);

                ui.separator();
                ui.label("確認項目");
                ui.label("・変換候補ウィンドウが入力位置（キャレット）の近くに出るか");
                ui.label("・未確定文字が欄の中に表示されるか。上のチェックを外すと下線が付くか");
                ui.label("・下線あり表示で、変換対象の文節が他の文節と区別できるか");
                ui.label("・確定した文字が欠けず、重複せず入るか");
                ui.label("・Esc / 変換 / 無変換 で候補操作ができ、欄からフォーカスが外れないか");
                ui.label("・WinRemap 常駐中に入力しても取りこぼしが起きないか");

                ui.separator();
                ui.label("直近の IME イベント（Preedit → Commit が出れば IME 経路が生きている）");
                if self.ime_events.is_empty() {
                    ui.label("（まだありません）");
                }
                for event in &self.ime_events {
                    ui.label(egui::RichText::new(event).monospace());
                }
            });
        });
    }
}
