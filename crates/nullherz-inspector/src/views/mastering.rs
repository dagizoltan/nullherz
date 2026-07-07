use egui::{Ui, Color32, Frame};
use crate::{InspectorApp, widgets};
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.heading("Precision Mastering");
    ui.add_space(20.0);

    ui.horizontal_top(|ui| {
        ui.set_min_width(400.0);
    Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(15.0).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut app.mastering_eq_enabled, "3-BAND EQ");
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if widgets::render_knob(ui, &mut app.mastering_eq_low, 0.0..=2.0, "LOW", app.theme.accent).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id: 19, param_id: 0, value: app.mastering_eq_low, ramp_duration_samples: 128 }));
                }
                if widgets::render_knob(ui, &mut app.mastering_eq_mid, 0.0..=2.0, "MID", app.theme.accent).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id: 19, param_id: 1, value: app.mastering_eq_mid, ramp_duration_samples: 128 }));
                }
                if widgets::render_knob(ui, &mut app.mastering_eq_high, 0.0..=2.0, "HIGH", app.theme.accent).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id: 19, param_id: 2, value: app.mastering_eq_high, ramp_duration_samples: 128 }));
                }
            });
        });
    });

    ui.add_space(20.0);

    ui.vertical(|ui| {
        ui.strong("Final Stage Analysis");
        ui.add_space(10.0);
        if let Some(t) = telemetry {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("Phase/Goniometer");
                    widgets::render_goniometer(ui, &t.goniometer_pts, 250.0, app.theme.accent);
                });
                ui.add_space(30.0);
                ui.vertical(|ui| {
                    ui.label("Master Spectrum");
                    widgets::render_spectrum_analyzer(ui, &t.spectrum, app.theme.accent, 120.0);
                });
            });
        }
    });
    });
}
