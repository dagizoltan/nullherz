use egui::{Ui, Frame, RichText};
use crate::InspectorApp;

pub fn render_calibration(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    ui.label(RichText::new("HARDWARE LATENCY CALIBRATION").small().strong().color(theme.text_secondary));
    ui.add_space(theme.space_xs);

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("Measure physical input-to-output Round-Trip Latency (RTL) via external loopback cable to achieve sample-accurate audio alignment.").color(theme.text_secondary));
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                if ui.button(RichText::new("● START CALIBRATION").strong().color(theme.accent)).on_hover_text("Emit test pulse and measure round-trip loopback delay").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CalibrateLatency));
                }

                ui.add_space(theme.space_md);

                if let Some(t) = app.last_telemetry.lock().as_ref() {
                    if t.calibration_samples > 0 {
                        let ms = t.calibration_samples as f32 / (t.sample_rate / 1000.0) ;
                        ui.label(RichText::new(format!("Current RTL: {:.1}ms ({} samples)", ms, t.calibration_samples)).strong().color(theme.success));
                    } else {
                        ui.label(RichText::new("Current RTL: Not Calibrated").color(theme.text_disabled));
                    }
                } else {
                    ui.label(RichText::new("Current RTL: --").color(theme.text_disabled));
                }
            });
        });

    ui.add_space(theme.space_md);
    ui.label(RichText::new("DISTRIBUTED CLOCK DISCIPLINE (PTP / IEEE 1588)").small().strong().color(theme.text_secondary));
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            if let Some(t) = app.last_telemetry.lock().as_ref() {
                ui.horizontal(|ui| {
                    ui.label("Sync Status:");
                    // "No clock installed" is not "perfectly locked". With no
                    // provider the jitter field sits at 0, which this panel used
                    // to read as flawless synchronisation and report as LOCKED.
                    if !t.clock_jitter_available {
                        ui.label(RichText::new("— NO CLOCK SOURCE").color(theme.text_secondary));
                    } else if t.clock_jitter_ns < 1000 {
                        ui.label(RichText::new("● LOCKED").color(theme.success));
                    } else {
                        ui.label(RichText::new("○ SEEKING").color(theme.warning));
                    }
                });
                ui.label(format!("System Time: {} ns", t.system_time_ns));
                ui.label(format!("Device Time: {} ns", t.device_time_ns));
                if t.clock_jitter_available {
                    ui.label(format!("Jitter: {} ns", t.clock_jitter_ns));
                } else {
                    ui.label(RichText::new("Jitter: not measured (no clock provider)").color(theme.text_secondary));
                }
                ui.label(format!("Offset: {} ns", (t.device_time_ns as i64 - t.system_time_ns as i64)));
            } else {
                ui.label("No clock telemetry available.");
            }
        });
}
