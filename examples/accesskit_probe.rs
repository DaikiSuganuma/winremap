//! Does a *deferred child viewport* get an AccessKit adapter? (v0.5 Phase A)
//!
//! WinRemap's real windows are child viewports declared by an invisible 1x1
//! host (ADR 0037), and `eframe` 0.35 initialises AccessKit only for
//! `ViewportId::ROOT` (`native/glow_integration.rs`). If that reading is
//! right, turning the `accesskit` feature on exposes the *host* to UI
//! Automation and leaves the windows that matter as opaque as before.
//!
//! This reproduces the arrangement in isolation so the answer does not depend
//! on driving WinRemap's tray:
//!
//! ```text
//! cargo run --example accesskit_probe
//! ```
//!
//! Then read the UIA tree of "WinRemap AccessKit Probe" from another shell.
//! A tree naming the button and the label means the reading was wrong and
//! Phase A is nearly free; only the title bar means the adapter has to be
//! taught about child viewports.

use eframe::egui;

fn main() -> eframe::Result<()> {
    // `-- root` is the control: the same widgets in an ordinary root
    // viewport. If UIA reads those but not the child's, the difference is
    // the viewport, not the feature or the renderer.
    if std::env::args().any(|arg| arg == "root") {
        return eframe::run_native(
            "winremap-accesskit-probe-root",
            eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_title("WinRemap AccessKit Probe Root")
                    .with_inner_size([420.0, 260.0]),
                ..Default::default()
            },
            Box::new(|_cc| Ok(Box::<RootProbe>::default())),
        );
    }

    // The same host as `gui::run_loop`: one pixel, undecorated, off-screen,
    // no taskbar button.
    let host = egui::ViewportBuilder::default()
        .with_title("winremap-probe-host")
        .with_inner_size([1.0, 1.0])
        .with_position([-32000.0, -32000.0])
        .with_decorations(false)
        .with_taskbar(false)
        .with_visible(false);

    eframe::run_native(
        "winremap-accesskit-probe",
        eframe::NativeOptions {
            viewport: host,
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::<Probe>::default())),
    )
}

/// The control: identical widgets, drawn straight into the root viewport.
#[derive(Default)]
struct RootProbe;

impl eframe::App for RootProbe {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("ProbeHeading");
            ui.label("ProbeLabel");
            if ui.button("ProbeButton").clicked() {
                println!("ProbeButton clicked");
            }
        });
    }
}

#[derive(Default)]
struct Probe {
    clicks: u32,
    settle_frames: u8,
}

impl eframe::App for Probe {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // eframe reveals the host after its first frame whatever the builder
        // said (ADR 0037), so hide it again for a few frames.
        if self.settle_frames < 3 {
            self.settle_frames += 1;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            ctx.request_repaint();
        }

        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("winremap-probe-child"),
            egui::ViewportBuilder::default()
                .with_title("WinRemap AccessKit Probe")
                .with_inner_size([420.0, 260.0]),
            {
                // The click counter proves an action arriving through UIA
                // reaches the widget, not just that the name is readable.
                let clicks = self.clicks;
                move |ui, _class| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.heading("ProbeHeading");
                        ui.label("ProbeLabel");
                        if ui.button("ProbeButton").clicked() {
                            println!("ProbeButton clicked");
                        }
                        ui.label(format!("clicks seen by the host: {clicks}"));
                    });
                }
            },
        );

        // A deferred viewport only lives while its parent keeps declaring it.
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}
