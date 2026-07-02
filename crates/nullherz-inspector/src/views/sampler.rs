use egui::{Color32, RichText, Ui, Frame, ScrollArea, Vec2, Sense, Stroke};
use crate::InspectorApp;
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        ui.heading("Production Sampler");
    });
    ui.add_space(10.0);

    Frame::none().fill(Color32::from_rgb(10, 10, 12)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
        ui.label("Sampler grid visualization...");
    });

    ui.add_space(20.0);
    ui.horizontal(|ui| {
        ui.heading("Loop Slicer");
        if ui.checkbox(&mut app.sampler_slicer_mode, "ENABLE").changed() {
             let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                target_id: 100,
                param_id: 3,
                value: if app.sampler_slicer_mode { 1.0 } else { 0.0 },
                ramp_duration_samples: 0,
            }));
        }
    });
}
