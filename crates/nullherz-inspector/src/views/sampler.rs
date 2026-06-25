use egui::{Color32, RichText, Ui, Frame, ScrollArea, Vec2, Sense, Stroke};
use crate::InspectorApp;
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        ui.heading("Production Sampler");
        ui.add_space(20.0);
        ui.label(RichText::new("GRID SEQUENCER (16x64)").color(Color32::from_gray(100)));
    });
    ui.add_space(10.0);

    if let Some(t) = telemetry {
        let samples_per_step = (44100.0 * 60.0 / app.global_bpm / 4.0) as u64;
        app.sequencer_active_step = (t.sample_counter / samples_per_step.max(1)) as usize % 64;
    } else {
         let time = ui.input(|i| i.time);
         app.sequencer_active_step = (time * 8.0) as usize % 64;
    }

    Frame::none().fill(Color32::from_rgb(10, 10, 12)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
        ScrollArea::both().show(ui, |ui| {
            ui.vertical(|ui| {
                for row in 0..16 {
                    ui.horizontal(|ui| {
                        ui.set_height(20.0);
                        let track_color = if row % 4 == 0 { Color32::from_rgb(0, 255, 200) } else { Color32::from_gray(60) };
                        ui.label(RichText::new(format!("TRK {:02}", row+1)).color(track_color).size(10.0).monospace());
                        ui.add_space(10.0);

                        for step in 0..64 {
                            let (rect, res) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::click());
                            if res.clicked() {
                                app.sequencer_grid[row][step] = !app.sequencer_grid[row][step];
                                let _ = app.command_sender.send(nullherz_traits::Command::SetSequencerStep {
                                    node_idx: 100,
                                    track: row as u32,
                                    step: step as u32,
                                    value: app.sequencer_grid[row][step],
                                });
                            }

                            let is_active = app.sequencer_grid[row][step];
                            let is_current = app.sequencer_active_step == step;

                            let mut bg_color = if is_active { track_color } else { Color32::from_gray(25) };
                            if is_current { bg_color = bg_color.additive(); }

                            let stroke = if is_current {
                                Stroke::new(2.0, Color32::WHITE)
                            } else if step % 4 == 0 {
                                Stroke::new(1.0, Color32::from_gray(40))
                            } else {
                                Stroke::new(0.5, Color32::from_gray(30))
                            };

                            ui.painter().rect_filled(rect, 1.0, bg_color);
                            ui.painter().rect_stroke(rect, 1.0, stroke);

                            if is_current {
                                ui.painter().rect_filled(rect.expand(1.0), 1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 40));
                            }
                        }
                    });
                    ui.add_space(2.0);
                }
            });
        });
    });

    ui.add_space(20.0);
    ui.group(|ui| {
        ui.strong("SEQUENCER SETTINGS");
        ui.horizontal(|ui| {
            ui.label("Steps: 64");
            ui.add_space(20.0);
            ui.label("Resolution: 1/16");
            ui.add_space(20.0);
            if ui.button("CLEAR ALL").clicked() {
                app.sequencer_grid = [[false; 64]; 16];
            }
        });
    });
}
