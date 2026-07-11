use egui::{Ui, RichText, Color32, Vec2};
use crate::InspectorApp;

use audio_core::Telemetry;

pub fn render_deck_performance(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    ui.vertical(|ui| {
        ui.label(RichText::new("HOT-CUES").size(theme.type_caption).color(theme.text_secondary));
        egui::Grid::new(format!("perf_grid_{}", i)).spacing([theme.space_xs, theme.space_xs]).show(ui, |ui| {
            for row in 0..4 {
                for col in 0..2 {
                    let j = row * 2 + col;
                    let btn = egui::Button::new(RichText::new(format!("{}", j + 1)).strong().size(theme.type_caption))
                        .min_size(Vec2::new(32.0, 24.0))
                        .fill(theme.bg_surface);

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
