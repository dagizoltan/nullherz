use egui::{Ui, Color32, Frame};
use crate::{InspectorApp, widgets};

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("System Metrics");
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        ui.label("Visualizer Damping:");
        ui.add(egui::Slider::new(&mut app.visualizer_damping, 0.01..=1.0).text(""));
    });

    ui.add_space(10.0);

    let telemetry = app.last_telemetry.lock().unwrap().clone();

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.vertical(|ui| {
            ui.strong("DSP Performance");
            ui.add_space(10.0);

            Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
                if let Some(t) = &telemetry {
                    let load = (t.process_time_ns as f32 / 1_000_000.0) / (256.0 / 44100.0 * 1000.0) * 100.0;
                    ui.label(format!("Engine Load: {:.1}%", load));
                    ui.label(format!("Block Time: {:.2}ms", t.process_time_ns as f32 / 1_000_000.0));
                    ui.label(format!("Peak Time: {:.2}ms", t.peak_process_time_ns as f32 / 1_000_000.0));
                    ui.label(format!("X-RUNS: {}", t.xrun_count));

                    ui.add_space(5.0);
                    ui.label("System Pressure (Last Overrun)");
                    let pressure_norm = (t.last_xrun_magnitude_ns as f32 / 1_000_000.0).clamp(0.0, 5.0) / 5.0;
                    let color = if pressure_norm > 0.8 { Color32::RED } else if pressure_norm > 0.4 { Color32::YELLOW } else { Color32::from_rgb(0, 255, 200) };
                    ui.add(egui::ProgressBar::new(pressure_norm).desired_width(200.0).fill(color).text(format!("{:.1}ms", t.last_xrun_magnitude_ns as f32 / 1_000_000.0)));
                } else {
                    ui.label("Waiting for engine telemetry...");
                }
            });

            ui.add_space(20.0);
            ui.strong("Spectral Analysis");
            ui.add_space(10.0);

            Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
                if telemetry.is_some() {
                    widgets::render_spectrum_analyzer(ui, &app.damped_spectrum, Color32::from_rgb(0, 255, 200), 120.0);
                } else {
                    ui.label("No spectral data available.");
                }
            });

            ui.add_space(20.0);
            ui.strong("Phase & Correlation");
            ui.add_space(10.0);

            Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(_t) = &telemetry {
                        widgets::render_goniometer(ui, &app.damped_goniometer, 200.0, Color32::from_rgb(0, 255, 200));
                        ui.add_space(20.0);
                        ui.vertical(|ui| {
                             ui.label("Master Out (L/R)");
                             ui.horizontal(|ui| {
                                 widgets::render_vu_meter(ui, app.damped_master_peaks[0], app.damped_master_peaks[0], Color32::WHITE, 160.0);
                                 widgets::render_vu_meter(ui, app.damped_master_peaks[1], app.damped_master_peaks[1], Color32::WHITE, 160.0);
                             });
                        });
                    } else {
                        ui.label("No phase data available.");
                    }
                });
            });

            ui.add_space(20.0);
            ui.strong("Neural Sound DNA (Latent space)");
            ui.add_space(10.0);

            Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
                if telemetry.is_some() {
                    ui.horizontal_wrapped(|ui| {
                        for i in 0..16 {
                            let val = app.damped_latent[i];
                            let size = 15.0;
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
                            let color = if val > 0.0 {
                                Color32::from_rgb((val * 255.0) as u8, 100, 200)
                            } else {
                                Color32::from_rgb(40, 40, 50)
                            };
                            ui.painter().rect_filled(rect, 2.0, color);
                        }
                    });
                } else {
                    ui.label("No DNA data available.");
                }
            });

            ui.add_space(20.0);
            ui.strong("Remote Node Health");
            ui.add_space(10.0);

            Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
                if let Some(t) = &telemetry {
                    if t.remote_node_count == 0 {
                        ui.label("No remote sidecars connected.");
                    } else {
                        for i in 0..(t.remote_node_count as usize).min(8) {
                            ui.horizontal(|ui| {
                                ui.label(format!("Node {}:", i));
                                ui.add(egui::ProgressBar::new(t.remote_cpu_usage[i] / 100.0).desired_width(100.0).text(format!("{:.1}% CPU", t.remote_cpu_usage[i])));
                                ui.label(format!("{:.1}ms Latency", t.remote_latency_ms[i]));
                            });
                        }
                    }
                }
            });
        });
    });
}
