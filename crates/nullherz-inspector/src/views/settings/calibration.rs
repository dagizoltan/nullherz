use egui::{Ui, Frame, RichText};
use crate::InspectorApp;

pub fn render_calibration(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("Hardware Latency Calibration");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label("Measure Round-Trip Latency (RTL) to ensure sample-accurate alignment.");
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                if ui.button(RichText::new("● START CALIBRATION").color(theme.accent)).clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CalibrateLatency));
                }

                if let Some(t) = app.last_telemetry.lock().unwrap().as_ref() {
                    if t.calibration_samples > 0 {
                        let ms = t.calibration_samples as f32 / (t.sample_rate / 1000.0) ;
                        ui.label(format!("Current RTL: {:.1}ms ({} samples)", ms, t.calibration_samples));
                    } else {
                        ui.label("Current RTL: Not Calibrated");
                    }
                } else {
                    ui.label("Current RTL: --");
                }
            });
        });

    ui.add_space(theme.space_md);
    ui.strong("Distributed Clock Discipline (PTP/IEEE 1588)");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            if let Some(t) = app.last_telemetry.lock().unwrap().as_ref() {
                ui.horizontal(|ui| {
                    ui.label("Sync Status:");
                    if t.clock_jitter_ns < 1000 {
                        ui.label(RichText::new("● LOCKED").color(theme.success));
                    } else {
                        ui.label(RichText::new("○ SEEKING").color(theme.warning));
                    }
                });
                ui.label(format!("System Time: {} ns", t.system_time_ns));
                ui.label(format!("Device Time: {} ns", t.device_time_ns));
                ui.label(format!("Jitter: {} ns", t.clock_jitter_ns));
                ui.label(format!("Offset: {} ns", (t.device_time_ns as i64 - t.system_time_ns as i64)));
            } else {
                ui.label("No clock telemetry available.");
            }
        });
}
