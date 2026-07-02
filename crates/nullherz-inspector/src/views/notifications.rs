use egui::{Ui, ScrollArea};
use crate::InspectorApp;

pub fn render(_app: &InspectorApp, ui: &mut Ui) {
    ui.heading("System Notifications");
    ui.add_space(10.0);

    ScrollArea::vertical().show(ui, |ui| {
        ui.vertical(|ui| {
            ui.label("• Engine initialized successfully.");
            ui.label("• ALSA backend connected at 48kHz.");
        });
    });
}
