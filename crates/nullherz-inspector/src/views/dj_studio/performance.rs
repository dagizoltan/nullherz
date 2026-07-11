use egui::{Ui, RichText, Color32, Vec2};
use crate::InspectorApp;
use nullherz_dna::GeneticLibrary;

use audio_core::Telemetry;

pub fn render_deck_performance(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    ui.vertical(|ui| {
        ui.label(RichText::new("HOT-CUES").small().color(Color32::from_gray(100)));

        // Retrieve track metadata to check if hotcue is set
        let track_id = app.now_playing[i];
        let track = track_id.and_then(|id| app.library_db.get_track(id).ok().flatten());

        egui::Grid::new(format!("perf_grid_{}", i)).spacing([4.0, 4.0]).show(ui, |ui| {
            for row in 0..4 {
                for col in 0..2 {
                    let j = row * 2 + col;

                    // Check if Hot Cue is set in metadata
                    let is_set = track.as_ref()
                        .map(|t| t.metadata.hot_cues[j].is_some())
                        .unwrap_or(false);

                    // High-gloss pad styling (glow color if set, dim dashed look if empty)
                    let (pad_bg, stroke_color) = if is_set {
                        (InspectorApp::deck_color(i).linear_multiply(0.3), InspectorApp::deck_color(i))
                    } else {
                        (Color32::from_gray(25), Color32::from_gray(45))
                    };

                    let btn = egui::Button::new(RichText::new(format!("{}", j + 1)).strong().size(11.0))
                        .min_size(Vec2::new(32.0, 24.0))
                        .fill(pad_bg)
                        .stroke(egui::Stroke::new(1.0, stroke_color));

                    let response = ui.add(btn);
                    let node_name = match i {
                        0 => "deck_a_sampler",
                        1 => "deck_b_sampler",
                        2 => "deck_c_sampler",
                        3 => "deck_d_sampler",
                        _ => "",
                    };
                    let node_idx = app.get_node_id(node_name);

                    if response.clicked() {
                        if ui.input(|i| i.modifiers.shift) {
                            let pos = telemetry.as_ref().map(|t| t.sample_counter).unwrap_or(0);
                            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::SetHotCue {
                                node_idx,
                                cue_idx: j as u32,
                                position_samples: pos,
                            }));
                        } else {
                            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpToHotCue {
                                node_idx,
                                cue_idx: j as u32,
                            }));
                        }
                    }
                }
                ui.end_row();
            }
        });
    });
}
