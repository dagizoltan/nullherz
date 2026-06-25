use egui::{Color32, Ui};
use crate::InspectorApp;
use crate::widgets;
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.heading("Studio Console");
    ui.add_space(20.0);
    ui.horizontal(|ui| {
        for i in 0..4 {
            ui.vertical_centered(|ui| {
                ui.strong(format!("CH {}", i + 1));
                let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1].min(1.2));
                if widgets::render_fader(ui, &mut app.channel_faders[i], 0.0..=1.2, InspectorApp::deck_color(i)).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::SetParam {
                        target_id: (i as u64 * 4 + 1),
                        param_id: 0,
                        value: app.channel_faders[i] * app.channel_trims[i],
                        ramp_duration_samples: 128,
                    });
                }
                widgets::render_vu_meter(ui, peak, peak, Color32::from_rgb(0, 255, 180), 120.0);
                ui.label(format!("{:.1} dB", 20.0 * peak.log10().max(-60.0)));
            });
            ui.add_space(50.0);
        }
    });
}
