use egui::{Color32, RichText, Ui, Frame, ScrollArea, Layout, Align};
use crate::InspectorApp;

pub fn render(app: &InspectorApp, ui: &mut Ui) {
    let telemetry = app.last_telemetry.lock().unwrap();

    ScrollArea::vertical().show(ui, |ui| {
        ui.vertical(|ui| {
            ui.heading("Engine Metrics");
            ui.add_space(10.0);

            if let Some(ref t) = *telemetry {
                Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical(|ui| {
                        let budget_ns = (128.0 / 44100.0) * 1e9;
                        let cpu_pct = (t.process_time_ns as f64 / budget_ns * 100.0).min(100.0);

                        ui.label(RichText::new("CPU LOAD").color(Color32::from_gray(150)).small().strong());
                        ui.label(RichText::new(format!("{:.2}%", cpu_pct))
                            .size(24.0)
                            .monospace()
                            .color(if cpu_pct > 80.0 { Color32::RED } else { Color32::from_rgb(0, 255, 200) }));

                        ui.add_space(10.0);
                        ui.label(RichText::new("X-RUNS").color(Color32::from_gray(150)).small().strong());
                        ui.label(RichText::new(format!("{:03}", t.xrun_count))
                            .size(24.0)
                            .monospace()
                            .color(if t.xrun_count > 0 { Color32::from_rgb(255, 150, 0) } else { Color32::from_gray(50) }));

                        ui.add_space(10.0);
                        ui.label(RichText::new("UPTIME (Samples)").color(Color32::from_gray(150)).small().strong());
                        ui.label(RichText::new(format!("{}", t.sample_counter)).monospace().color(Color32::from_gray(100)));
                    });
                });

                ui.add_space(20.0);
                ui.heading("I/O Peaks");
                ui.add_space(10.0);

                for i in 0..8 {
                    let peak = t.peak_levels[i];
                    let db = 20.0 * peak.log10().max(-60.0);

                    ui.horizontal(|ui| {
                        ui.set_width(ui.available_width());
                        ui.label(format!("CH {}", i + 1));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(format!("{:.1} dB", db)).monospace().color(if peak > 1.0 { Color32::RED } else { Color32::from_gray(100) }));
                        });
                    });
                    ui.add_space(2.0);
                }
            } else {
                ui.label(RichText::new("No Telemetry Data").color(Color32::from_gray(100)).italics());
            }
        });
    });
}
