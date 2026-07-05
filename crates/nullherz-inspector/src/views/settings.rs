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
                 if ui.add(egui::DragValue::new(&mut app.global_bpm).clamp_range(40.0..=300.0)).changed() {
                     let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetBpm(app.global_bpm)));
                 }
             });

             if ui.checkbox(&mut app.quantize_enabled, "Quantize Commands").changed() {
                  let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetSafeMode(app.quantize_enabled)));
             }

             ui.add_space(10.0);
             ui.strong("Controller Mapping");
             ui.horizontal(|ui| {
                 // In a real app, we would scan the 'mappings/' directory.
                 // For the Alpha demo, we'll provide these as standard options.
                 let options = ["default", "pioneer_ddj400"];
                 for opt in options {
                     if ui.button(format!("Load {}", opt)).clicked() {
                         let mut buffer = [0u8; 32];
                         let bytes = opt.as_bytes();
                         let len = bytes.len().min(32);
                         buffer[..len].copy_from_slice(&bytes[..len]);
                         let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::LoadMidiMap(buffer)));
                     }
                 }
             });
        });
    });

    ui.add_space(20.0);
    if ui.button("STOP ENGINE").clicked() {
        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop));
    }
}
