use egui::{Ui, Color32, Frame, Margin, Rounding, Stroke, RichText};
use crate::{InspectorApp, widgets};

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("System Metrics & Telemetry");
    ui.add_space(10.0);

    let telemetry = app.last_telemetry.lock().unwrap().clone();
    let frame_width = ui.available_width().min(400.0);

    egui::ScrollArea::vertical().id_source("metrics_scroll").show(ui, |ui| {
        ui.vertical(|ui| {
            // 1. Performance Section
            render_metric_group(ui, "DSP EXECUTION PLANE", frame_width, |ui| {
                if let Some(t) = &telemetry {
                    let load = (t.process_time_ns as f32 / 1_000_000.0) / (256.0 / 44100.0 * 1000.0) * 100.0;
                    ui.label(format!("Engine Load: {:.1}%", load));
                    ui.label(format!("X-RUNS: {}", t.xrun_count));

                    let pressure_norm = (t.last_xrun_magnitude_ns as f32 / 1_000_000.0).clamp(0.0, 5.0) / 5.0;
                    ui.add(egui::ProgressBar::new(pressure_norm).fill(Color32::from_rgb(0, 255, 200)).text("PRESSURE"));
                } else {
                    ui.label("No Telemetry Connection");
                }
            });

            ui.add_space(10.0);

            // 2. Analysis Section
            render_metric_group(ui, "SPECTRAL DOMAIN", frame_width, |ui| {
                if telemetry.is_some() {
                    widgets::render_spectrum_analyzer(ui, &app.damped_spectrum, Color32::from_rgb(0, 255, 200), 100.0);
                }
            });

            ui.add_space(10.0);

            render_metric_group(ui, "PHASE & CORRELATION", frame_width, |ui| {
                if telemetry.is_some() {
                    widgets::render_goniometer(ui, &app.damped_goniometer, 180.0, Color32::from_rgb(0, 255, 200));
                }
            });

            ui.add_space(10.0);

            // 3. Thread Heatmap (Mocked)
            render_metric_group(ui, "ORCHESTRATION THREADS", frame_width, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for i in 0..16 {
                        let val = (i as f32 * 0.1).sin() * 0.5 + 0.5;
                        let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                        let color = if val > 0.8 { Color32::RED } else if val > 0.4 { Color32::from_rgb(0, 150, 255) } else { Color32::from_gray(50) };
                        ui.painter().rect_filled(r, 1.0, color);
                    }
                });
            });
        });
    });
}

fn render_metric_group<F>(ui: &mut Ui, title: &str, width: f32, add_contents: F)
where F: FnOnce(&mut Ui)
{
    ui.label(RichText::new(title).small().strong().color(Color32::from_gray(120)));
    Frame::none()
        .fill(Color32::from_rgb(18, 18, 22))
        .rounding(Rounding::same(4.0))
        .stroke(Stroke::new(1.0, Color32::from_gray(30)))
        .inner_margin(Margin::same(10.0))
        .show(ui, |ui| {
            ui.set_width(width);
            add_contents(ui);
        });
}
