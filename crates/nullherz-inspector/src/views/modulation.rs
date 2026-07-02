use egui::{Ui, Color32, RichText};
use crate::InspectorApp;
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, _telemetry: &Option<Telemetry>) {
    ui.heading("Spectral Modulation Matrix");
    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label(RichText::new("ANAWAVES TRAIT INHERITANCE").color(Color32::from_gray(100)));
        ui.add_space(5.0);
        ui.vertical(|ui| {
            ui.label("Global Spectral Window");
            let prev_shape = app.spectral_window_shape;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut app.spectral_window_shape, 0, "Hann");
                ui.selectable_value(&mut app.spectral_window_shape, 1, "Hamming");
                ui.selectable_value(&mut app.spectral_window_shape, 2, "Blackman");
                ui.selectable_value(&mut app.spectral_window_shape, 3, "Rectangular");
            });

            if app.spectral_window_shape != prev_shape {
                 let _ = app.command_sender.send(nullherz_traits::Command::Extension(nullherz_traits::OpaqueEnvelope {
                    domain_id: 0x53504543, // "SPEC"
                    target_id: 100, // Assuming spectral morph node
                    opcode: 0x01,
                    data: [app.spectral_window_shape as u8, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                }));
            }
        });
    });

    ui.add_space(20.0);
    ui.label("Active Modulation Mappings");
    // List mappings and allow removal
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal(|ui| {
             ui.label("Macro 1 -> Filter Cutoff");
             if ui.button("REMOVE").clicked() {
                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::RemoveModMapping {
                    macro_id: 0,
                    target_id: 1,
                    param_id: 0,
                }));
             }
        });
    });
}
