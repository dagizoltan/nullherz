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

    ui.add_space(20.0);
    ui.separator();
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        ui.heading("Loop Slicer");
        ui.add_space(20.0);
        if ui.checkbox(&mut app.sampler_slicer_mode, "ENABLE SLICER").changed() {
            let _ = app.command_sender.send(nullherz_traits::Command::SetParam {
                target_id: 100, // Assuming node 100 is the sampler
                param_id: 3,
                value: if app.sampler_slicer_mode { 1.0 } else { 0.0 },
                ramp_duration_samples: 0,
            });
        }
        ui.add_space(20.0);
        ui.label("Grid:");
        let old_grid = app.sampler_slice_grid;
        egui::ComboBox::from_id_source("slicer_grid")
            .selected_text(match app.sampler_slice_grid {
                1.0 => "1/4",
                0.5 => "1/8",
                0.25 => "1/16",
                _ => "Custom",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.sampler_slice_grid, 1.0, "1/4");
                ui.selectable_value(&mut app.sampler_slice_grid, 0.5, "1/8");
                ui.selectable_value(&mut app.sampler_slice_grid, 0.25, "1/16");
            });

        if app.sampler_slice_grid != old_grid {
            let _ = app.command_sender.send(nullherz_traits::Command::SetParam {
                target_id: 100,
                param_id: 4,
                value: app.sampler_slice_grid,
                ramp_duration_samples: 0,
            });
        }
    });

    ui.add_space(10.0);

    let mut current_slice = None;
    if let Some(t) = telemetry {
         let samples_per_beat = (44100.0 * 60.0 / app.global_bpm.max(1.0)) as f64;
         let beats_per_slice = app.sampler_slice_grid as f64;
         let samples_per_slice = samples_per_beat * beats_per_slice;
         if samples_per_slice > 0.0 {
             current_slice = Some(((t.sample_counter as f64 / samples_per_slice) as u32) % 16);
         }
    }

    // 4x4 Slice Pads
    Frame::none().fill(Color32::from_rgb(15, 15, 18)).rounding(4.0).inner_margin(10.0).show(ui, |ui| {
        egui::Grid::new("slicer_pads").spacing(Vec2::new(10.0, 10.0)).show(ui, |ui| {
            for row in 0..4 {
                for col in 0..4 {
                    let slice_idx = (row * 4 + col) as u32;
                    let (rect, res) = ui.allocate_exact_size(Vec2::new(80.0, 80.0), Sense::click());

                    let is_active = app.sampler_slicer_mode;
                    let is_playing = current_slice == Some(slice_idx);

                    let mut color = if is_active {
                        if is_playing {
                            Color32::from_rgb(0, 200, 150)
                        } else {
                            Color32::from_rgb(40, 40, 50)
                        }
                    } else {
                        Color32::from_gray(20)
                    };

                    if res.clicked() && is_active {
                        let _ = app.command_sender.send(nullherz_traits::Command::TriggerSlice {
                            node_idx: 100,
                            slice_idx,
                        });
                        color = Color32::from_rgb(0, 255, 200);
                    } else if res.hovered() && is_active {
                        color = Color32::from_rgb(60, 60, 80);
                    }

                    let stroke_color = if is_playing { Color32::WHITE } else { Color32::from_gray(50) };
                    let stroke_width = if is_playing { 2.0 } else { 1.0 };

                    ui.painter().rect_filled(rect, 4.0, color);
                    ui.painter().rect_stroke(rect, 4.0, Stroke::new(stroke_width, stroke_color));
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, format!("{}", slice_idx + 1), egui::FontId::proportional(18.0), Color32::WHITE);
                }
                ui.end_row();
            }
        });
    });
}
