use egui::{Ui, Color32, Frame};
use crate::{InspectorApp, widgets};

pub fn render(_app: &InspectorApp, ui: &mut Ui) {
    ui.heading("System Metrics");
    ui.add_space(20.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.vertical(|ui| {
            ui.strong("DSP Performance");
            ui.add_space(10.0);

            Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
                ui.label("Average Cycle Load: 45%");
                ui.label("Max Jitter: 1.2ms");
            });

            ui.add_space(20.0);
            ui.strong("Visualizers");
            ui.add_space(10.0);

            Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
                ui.label("Spectrum Analyzer [Placeholder]");
                widgets::render_vu_meter(ui, 0.5, 1.0, Color32::WHITE, 200.0);
            });
        });
    });
}
