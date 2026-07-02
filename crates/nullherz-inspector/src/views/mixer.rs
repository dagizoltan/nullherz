use egui::{Ui, Color32};
use crate::{InspectorApp, widgets};
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.heading("System Mixer");
    ui.add_space(20.0);

    ui.horizontal(|ui| {
        for i in 0..4 {
            ui.vertical(|ui| {
                ui.label(format!("CH {}", i + 1));
                let color = InspectorApp::deck_color(i);
                if widgets::render_fader(ui, &mut app.channel_faders[i], 0.0..=1.2, color, 120.0, 30.0).changed() {
                    let target_id = (10 + i) as u64; // Example mapping
                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                        target_id,
                        param_id: 0,
                        value: app.channel_faders[i],
                        ramp_duration_samples: 128,
                    }));
                }

                if let Some(t) = telemetry {
                    let level = t.peak_levels[i];
                    widgets::render_vu_meter(ui, level, 1.2, color, 100.0);
                }
            });
            ui.add_space(10.0);
        }
    });
}
