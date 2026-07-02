use egui::{Ui, Color32, Frame};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("System Settings");
    ui.add_space(20.0);

    Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(15.0).show(ui, |ui| {
        ui.vertical(|ui| {
             ui.strong("Global Audio");
             ui.horizontal(|ui| {
                 ui.label("BPM:");
                 ui.add(egui::DragValue::new(&mut app.global_bpm).clamp_range(40.0..=300.0));
             });

             if ui.checkbox(&mut app.quantize_enabled, "Quantize Commands").changed() {
                  let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetSafeMode(app.quantize_enabled)));
             }
        });
    });

    ui.add_space(20.0);
    if ui.button("STOP ENGINE").clicked() {
        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop));
    }
}
