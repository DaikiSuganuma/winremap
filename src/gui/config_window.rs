//! The config window (root viewport).
//!
//! Milestone B0 only carries what the tray gave up: the config file's path and
//! a button to open it in the user's text editor. The editing UI arrives in
//! B1-B4 (docs/v0.2/04_config-gui-design.md §7); keeping B0 empty is
//! deliberate, so the viewport rework can be verified against the Phase A
//! checklist before any of it is built on.

use std::path::Path;

use eframe::egui;

use crate::i18n;

#[derive(Default)]
pub struct ConfigWindow {}

impl ConfigWindow {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let texts = i18n::t();
        let path = super::config_path()
            .lock()
            .map(|path| path.clone())
            .unwrap_or_default();

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(8.0);
            ui.heading(texts.config_window_title);
            ui.add_space(8.0);
            ui.label(texts.config_window_placeholder);
            ui.add_space(16.0);

            ui.label(texts.config_window_file);
            ui.label(egui::RichText::new(path.display().to_string()).monospace());
            ui.add_space(8.0);
            if ui.button(texts.config_window_open_in_editor).clicked() {
                open_in_default_editor(&path);
            }
        });
    }
}

/// Hands the file to whatever the user associated with `.toml`. Moved here
/// from the tray when the menu item became "Settings" (owner decision
/// 2026-07-21).
fn open_in_default_editor(path: &Path) {
    // `start` defers to the user's .toml file association; the empty string
    // fills start's window-title slot so the path is not mistaken for it.
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", &path.to_string_lossy()])
        .spawn();
}
