use egui::Ui;
use crate::InspectorApp;

pub fn render(_app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("Audio Editor");
    ui.add_space(20.0);
    ui.label("Waveform editing and destructive processing coming soon...");
}
