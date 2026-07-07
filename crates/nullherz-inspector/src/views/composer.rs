use egui::{Ui, ScrollArea, Color32, Vec2, Sense, RichText, Stroke};
pub use nullherz_conductor::pattern_manager::DnaSequencer;
use crate::InspectorApp;
use audio_core::Telemetry;
use nullherz_traits::{Command, PerformanceCommand};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        ui.heading("Track Composer");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
             ui.label(egui::RichText::new("QUANTIZED: 1 BAR").color(Color32::from_rgb(0, 255, 200)).size(10.0));
        });
    });
    ui.add_space(20.0);

    ui.horizontal(|ui| {
        let is_recording = app.record_automation;
        ui.toggle_value(&mut app.record_automation, RichText::new("🔴 RECORD AUTOMATION").color(if is_recording { Color32::RED } else { Color32::GRAY }));
        ui.add_space(20.0);
        if ui.button("CLEAR ALL PATTERNS").clicked() {
            for i in 0..16 {
                 let _ = app.command_sender.send(Command::Performance(PerformanceCommand::ClearTrackPattern { node_idx: 70, track_idx: i as u32 }));
                 app.sequencer_grid[i].fill(0.0);
            }
        }
    });
    ui.add_space(10.0);

    ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_top(|ui| {
            // Left margin / Track Labels
            ui.vertical(|ui| {
                ui.add_space(40.0); // Offset for header
                for i in 0..16 {
                    ui.group(|ui| {
                        ui.set_min_size(Vec2::new(120.0, 60.0));

                            // Evolution Feedback (Track Label pulsing)
                            let time = ui.input(|i| i.time);
                            let pulse = (time * 5.0).sin() as f32 * 0.5 + 0.5;
                            let is_evolving = i % 4 == 0; // Mock condition: track 1, 5, 9, 13 are "evolving"
                            let label_color = if is_evolving {
                                Color32::from_rgb(0, 255, 200).gamma_multiply(0.5 + pulse * 0.5)
                            } else {
                                Color32::WHITE
                            };

                            ui.label(RichText::new(format!("TRACK {}", i + 1)).strong().color(label_color));
                        ui.horizontal(|ui| {
                            if ui.selectable_label(app.track_mutes[i], "M").on_hover_text("Mute").clicked() {
                                app.track_mutes[i] = !app.track_mutes[i];
                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetTrackMute { node_idx: 70, track_idx: i as u32, muted: app.track_mutes[i] }));
                            }
                            if ui.selectable_label(app.track_solos[i], "S").on_hover_text("Solo").clicked() {
                                app.track_solos[i] = !app.track_solos[i];
                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetTrackSolo { node_idx: 70, track_idx: i as u32, soloed: app.track_solos[i] }));
                            }
                            if ui.button("C").on_hover_text("Clear").clicked() {
                                app.sequencer_grid[i].fill(0.0);
                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::ClearTrackPattern { node_idx: 70, track_idx: i as u32 }));
                            }
                        });
                    });
                    ui.add_space(8.0);
                }
            });

            // Grid with Horizontal Scrolling & Column-based Culling
            let cell_size = 60.0;
            let spacing = 8.0;

            egui::ScrollArea::horizontal().id_source("composer_grid_h").show(ui, |ui| {
                ui.vertical(|ui| {
                    let view_rect = ui.clip_rect();
                    let start_x = ui.cursor().min.x;
                    let start_col = ((view_rect.left() - start_x) / (cell_size + spacing)).floor().max(0.0) as usize;
                    let end_col = ((view_rect.right() - start_x) / (cell_size + spacing)).ceil().min(64.0) as usize;

                    // Timeline Ruler
                    ui.horizontal(|ui| {
                        ui.add_space(start_col as f32 * (cell_size + spacing));
                        for i in start_col..end_col {
                            let is_bar = i % 4 == 0;
                            let text = if is_bar { format!("{}", (i / 4) + 1) } else { "".to_string() };
                            let (rect, _) = ui.allocate_exact_size(Vec2::new(cell_size, 20.0), Sense::hover());

                            if is_bar {
                                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::monospace(12.0), Color32::from_gray(180));
                            }
                            ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, if is_bar { Color32::from_gray(100) } else { Color32::from_gray(50) }));
                            ui.add_space(spacing);
                        }
                        ui.add_space((64 - end_col) as f32 * (cell_size + spacing));
                    });

                    // Column Headers (Scene Launchers)
                    ui.horizontal(|ui| {
                        ui.add_space(start_col as f32 * (cell_size + spacing));
                        for i in start_col..end_col {
                            if ui.add_sized([cell_size, 22.0], egui::Button::new(RichText::new(format!("S{}", i + 1)).small()).fill(Color32::from_gray(40))).clicked() {
                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::LaunchClip { row: 0xFF, col: i as u32 }));
                            }
                            ui.add_space(spacing);
                        }
                        ui.add_space((64 - end_col) as f32 * (cell_size + spacing));
                    });

                    ui.add_space(10.0);

                    for row in 0..16 {
                        ui.horizontal(|ui| {
                            // Pre-calculate visible range for the inner loop
                            ui.add_space(start_col as f32 * (cell_size + spacing));

                            for col in start_col..end_col {
                                let (rect, response) = ui.allocate_exact_size(Vec2::new(cell_size, cell_size), Sense::click());

                                // True Visual State from Telemetry
                                let mut is_playing = false;
                                let mut is_starting = false;

                                if let Some(t) = telemetry {
                                    is_playing = t.active_clips[row] == col as u8;
                                    is_starting = (t.starting_clips_mask[row] >> col) & 1 == 1;
                                }

                                let velocity = app.sequencer_grid[row][col];
                                let mut color = if is_playing {
                                    Color32::from_rgb(0, 255, 100)
                                } else if is_starting {
                                    Color32::from_rgb(255, 200, 0)
                                } else if velocity > 0.0 {
                                    Color32::from_rgb(0, 150, 255).gamma_multiply(velocity.clamp(0.3, 1.0))
                                } else {
                                    Color32::from_gray(30)
                                };

                                if col == app.sequencer_active_step {
                                    color = color.linear_multiply(1.5);
                                }

                                ui.painter().rect_filled(rect, 2.0, color);

                                if is_playing {
                                     ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(2.0, Color32::WHITE));
                                }
                                if response.hovered() {
                                    ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, Color32::WHITE));
                                }

                                if response.clicked() {
                                    let is_on = app.sequencer_grid[row][col] == 0.0;
                                    let val = if is_on { 1.0 } else { 0.0 };
                                    app.sequencer_grid[row][col] = val;
                                    let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetSequencerStep {
                                        node_idx: 70,
                                        track: row as u32,
                                        step: col as u32,
                                        value: val,
                                    }));
                                }

                                if response.dragged() {
                                    let delta = response.drag_delta().y * -0.01;
                                    let new_val = (app.sequencer_grid[row][col] + delta).clamp(0.01, 1.0);
                                    app.sequencer_grid[row][col] = new_val;
                                    let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetSequencerStep {
                                        node_idx: 70,
                                        track: row as u32,
                                        step: col as u32,
                                        value: new_val,
                                    }));
                                }
                                ui.add_space(spacing);
                            }
                            ui.add_space((64 - end_col) as f32 * (cell_size + spacing));

                            // DNA Control Sidebar for Row
                            ui.vertical(|ui| {
                                 ui.set_max_width(40.0);
                                 if ui.add_sized([35.0, 30.0], egui::Button::new("🧬")).on_hover_text("Transfuse DNA across row").clicked() {
                                      let _ = app.command_sender.send(Command::Performance(PerformanceCommand::TransfuseRow { row: row as u32 }));
                                 }
                                 ui.add_space(4.0);
                                 let mut strength = 0.0;
                                 if ui.add(egui::Slider::new(&mut strength, 0.0..=1.0).vertical().show_value(false)).on_hover_text("Genetic Evolution Strength").changed() {
                                      let _ = app.command_sender.send(Command::Performance(PerformanceCommand::EvolvePattern {
                                          node_idx: row as u32,
                                          track_idx: 0,
                                          mutation_strength: strength,
                                      }));
                                 }
                            });
                        });
                        ui.add_space(8.0);
                    }
                });
            });
        });
    });
}
