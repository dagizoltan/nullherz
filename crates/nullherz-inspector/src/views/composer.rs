use egui::{Ui, ScrollArea};
use crate::InspectorApp;

pub fn render(_app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("Track Composer");
    ui.add_space(20.0);

    ScrollArea::vertical().show(ui, |ui| {
        ui.label("Timeline arrangement view coming soon.");
        ui.add_space(10.0);
        ui.group(|ui| {
            ui.label("TRACK 1: [ EMPTY ]");
        });
    });
}
