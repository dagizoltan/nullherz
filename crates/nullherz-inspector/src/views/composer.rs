use egui::{Ui, ScrollArea, Color32, Vec2, Sense};
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

    ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_top(|ui| {
            // Left margin / Track Labels
            ui.vertical(|ui| {
                ui.add_space(40.0); // Offset for header
                for i in 0..8 {
                    ui.add_sized([80.0, 60.0], egui::Label::new(format!("TRACK {}", i + 1)));
                    ui.add_space(8.0);
                }
            });

            // Grid
            ui.vertical(|ui| {
                // Column Headers
                ui.horizontal(|ui| {
                    for i in 0..8 {
                        ui.add_sized([60.0, 30.0], egui::Label::new(format!("S{}", i + 1)));
                        ui.add_space(8.0);
                    }
                });

                for row in 0..8 {
                    ui.horizontal(|ui| {
                        for col in 0..8 {
                            let (rect, response) = ui.allocate_exact_size(Vec2::new(60.0, 60.0), Sense::click());

                            // True Visual State from Telemetry
                            let mut is_playing = false;
                            let mut is_starting = false;

                            if let Some(t) = telemetry {
                                is_playing = t.active_clips[row] == col as u8;
                                is_starting = (t.starting_clips_mask[row] >> col) & 1 == 1;
                            }

                            let color = if is_playing {
                                Color32::from_rgb(0, 255, 100)
                            } else if is_starting {
                                // Pulsing animation logic would go here
                                Color32::from_rgb(255, 200, 0)
                            } else {
                                Color32::from_gray(30)
                            };

                            ui.painter().rect_filled(rect, 2.0, color);

                            if is_playing {
                                 // Add playing indicator (glow)
                                 ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(2.0, Color32::WHITE));
                            }
                            if response.hovered() {
                                ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, Color32::WHITE));
                            }

                            if response.clicked() {
                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::LaunchClip {
                                    row: row as u32,
                                    col: col as u32,
                                }));
                            }
                            ui.add_space(8.0);
                        }

                        // Macro Button for Transfusion
                        if ui.add_sized([30.0, 60.0], egui::Button::new("🧬")).on_hover_text("Transfuse DNA across row").clicked() {
                             let _ = app.command_sender.send(Command::Performance(PerformanceCommand::TransfuseRow {
                                row: row as u32,
                            }));
                        }
                    });
                    ui.add_space(8.0);
                }
            });
        });
    });
}
