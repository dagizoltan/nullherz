use egui::{Ui, Color32, Frame, Margin, Rounding, Stroke, RichText, Sense};
use crate::{InspectorApp, widgets};

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("System Metrics & Telemetry");
    ui.add_space(10.0);

    let telemetry = app.last_telemetry.lock().unwrap().clone();
    let frame_width = ui.available_width().min(400.0);
    let theme = app.theme;

    egui::ScrollArea::vertical().id_source("metrics_scroll").show(ui, |ui| {
        ui.vertical(|ui| {
            // 1. Performance Section
            render_metric_group(ui, "DSP EXECUTION PLANE", frame_width, &theme, |ui| {
                if let Some(t) = &telemetry {
                    let load = (t.process_time_ns as f32 / 1_000_000.0) / (256.0 / 44100.0 * 1000.0) * 100.0;
                    ui.label(format!("Engine Load: {:.1}%", load));
                    ui.label(format!("X-RUNS: {}", t.xrun_count));
                    ui.label(format!("Resource Leaks: {}", t.resource_leaks));

                    let pressure_norm = (t.last_xrun_magnitude_ns as f32 / 1_000_000.0).clamp(0.0, 5.0) / 5.0;
                    ui.add(egui::ProgressBar::new(pressure_norm).fill(theme.accent).text("PRESSURE"));

                    ui.add_space(5.0);
                    ui.label(RichText::new("NODE PERFORMANCE BREAKDOWN").small().color(theme.text_secondary));
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), Sense::hover());
                    let node_w = rect.width() / 64.0;
                    for i in 0..64 {
                        let time = t.node_times_ns[i] as f32 / 100_000.0; // Scaled
                        let h = time.clamp(1.0, 30.0);
                        let r = egui::Rect::from_min_max(egui::pos2(rect.left() + i as f32 * node_w, rect.bottom() - h), egui::pos2(rect.left() + (i+1) as f32 * node_w - 1.0, rect.bottom()));
                        ui.painter().rect_filled(r, 0.0, theme.track_colors[1]);
                    }
                } else {
                    ui.label("No Telemetry Connection");
                }
            });

            ui.add_space(10.0);

            // 2. Analysis Section
            render_metric_group(ui, "SPECTRAL DOMAIN", frame_width, &theme, |ui| {
                if telemetry.is_some() {
                    widgets::render_spectrum_analyzer(ui, &app.damped_spectrum, theme.accent, 100.0);
                }
            });

            ui.add_space(10.0);

            render_metric_group(ui, "DNA LATENT SPACE PROJECTION", frame_width, &theme, |ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(frame_width, 150.0), Sense::hover());
                ui.painter().rect_filled(rect, theme.radius_md, theme.bg_dark.linear_multiply(0.8));

                let center = rect.center();
                let scale = 60.0;

                // Project 16D latent space to 2D using a simple fixed projection
                let mut x = 0.0;
                let mut y = 0.0;
                for i in 0..16 {
                    let angle = (i as f32 / 16.0) * std::f32::consts::PI * 2.0;
                    x += app.damped_latent[i] * angle.cos();
                    y += app.damped_latent[i] * angle.sin();
                }

                let pos = center + egui::vec2(x * scale, y * scale);
                ui.painter().circle_filled(pos, 6.0, theme.accent);
                ui.painter().circle_stroke(pos, 8.0, Stroke::new(1.0, theme.text_primary));

                ui.add_space(5.0);
                ui.label(RichText::new("TIMBRAL TRAJECTORY").small().color(theme.text_secondary));
            });

            ui.add_space(10.0);

            render_metric_group(ui, "PHASE & CORRELATION", frame_width, &theme, |ui| {
                if telemetry.is_some() {
                    widgets::render_goniometer(ui, &app.damped_goniometer, 180.0, theme.accent);
                }
            });

            ui.add_space(10.0);

            // 3. Distributed Execution Section
            render_metric_group(ui, "DISTRIBUTED EXECUTION (REMOTE NODES)", frame_width, &theme, |ui| {
                if let Some(t) = &telemetry {
                    if t.remote_node_count == 0 {
                        ui.label(RichText::new("No remote nodes attached.").italics().small().color(theme.text_secondary));
                    } else {
                        for i in 0..(t.remote_node_count as usize).min(8) {
                            ui.horizontal(|ui| {
                                ui.label(format!("Node {}", i));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(format!("{:.1}ms", t.remote_latency_ms[i]));
                                    ui.add(egui::ProgressBar::new(t.remote_cpu_usage[i] / 100.0).desired_width(100.0).text(format!("{:.1}%", t.remote_cpu_usage[i])));
                                });
                            });
                        }
                    }
                }
            });

            ui.add_space(10.0);

            // 4. Thread Heatmap (Grounded)
            render_metric_group(ui, "EXECUTION PLANE THREADS (WORKERS)", frame_width, &theme, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let worker_count = nullherz_traits::DEFAULT_WORKER_COUNT;

                    for i in 0..worker_count {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());

                        // Color based on engine load as a proxy for thread activity
                        let load = if let Some(t) = &telemetry {
                            (t.process_time_ns as f32 / 1_000_000.0) / (256.0 / 44100.0 * 1000.0)
                        } else {
                            0.0
                        };

                        let color = if load > 0.9 {
                            Color32::from_rgb(255, 50, 50) // Stress
                        } else if load > 0.1 {
                            theme.accent.gamma_multiply(0.8) // Active
                        } else {
                            theme.bg_inset // Idle
                        };

                        ui.painter().rect_filled(r, 2.0, color);
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, format!("{}", i), egui::FontId::monospace(10.0), theme.text_primary);
                    }
                });
                ui.add_space(5.0);
                ui.label(RichText::new("STABLE PARALLEL STAGE EXECUTION").small().color(theme.text_secondary));
            });
        });
    });
}

fn render_metric_group<F>(ui: &mut Ui, title: &str, width: f32, theme: &nullherz_ui_hal::Theme, add_contents: F)
where F: FnOnce(&mut Ui)
{
    ui.label(RichText::new(title).small().strong().color(theme.text_secondary));
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .stroke(theme.border)
        .inner_margin(Margin::same(theme.space_md))
        .show(ui, |ui| {
            ui.set_width(width);
            add_contents(ui);
        });
}
