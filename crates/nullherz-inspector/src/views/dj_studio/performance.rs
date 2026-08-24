use egui::{Ui, RichText, Vec2};
use crate::InspectorApp;

use audio_core::Telemetry;

pub fn render_deck_performance(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let track = app.decks.cached_tracks[i].clone();

    ui.vertical(|ui| {
        ui.label(RichText::new("HOT-CUES & LOOPS").size(theme.type_caption).color(theme.text_secondary));

        let node_name = match i {
            0 => "deck_a_sampler",
            1 => "deck_b_sampler",
            2 => "deck_c_sampler",
            3 => "deck_d_sampler",
            _ => "",
        };
        let node_idx = app.get_node_id(node_name);

        egui::Grid::new(format!("perf_grid_{}", i)).spacing([theme.space_xs, theme.space_xs]).show(ui, |ui| {
            for row in 0..2 {
                for col in 0..4 {
                    let j = row * 4 + col;
                    let cue_set = track.as_ref().and_then(|t| t.metadata.hot_cues[j]);
                    let fill_color = if cue_set.is_some() {
                        theme.accent.linear_multiply(0.3)
                    } else {
                        theme.bg_surface
                    };
                    let text_color = if cue_set.is_some() {
                        theme.accent
                    } else {
                        theme.text_primary
                    };

                    let label = if cue_set.is_some() {
                        format!("{}★", j + 1)
                    } else {
                        format!("{}", j + 1)
                    };

                    let btn = egui::Button::new(RichText::new(label).strong().size(theme.type_caption).color(text_color))
                        .min_size(Vec2::new(28.0, 24.0))
                        .fill(fill_color);

                    let response = ui.add(btn);

                    if let (true, Some(node_idx)) = (response.clicked(), node_idx) {
                        if ui.input(|inp| inp.modifiers.shift) {
                            let pos = telemetry.as_ref().map(|t| t.deck_positions[i]).unwrap_or(0);
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

        ui.add_space(theme.space_xs);

        // Beat Loop Controls Row
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.space_xs;
            let loop_beats = [1.0f32, 2.0, 4.0, 8.0];
            for beats in loop_beats {
                let btn = egui::Button::new(RichText::new(format!("{}B", beats as u32)).size(theme.type_caption).strong())
                    .min_size(Vec2::new(22.0, 20.0))
                    .fill(theme.bg_surface);
                if ui.add(btn).clicked() {
                    if let (Some(node_idx), Some(t)) = (node_idx, &track) {
                        let sample_rate = t.metadata.sample_rate.max(1) as f32;
                        let bpm = t.metadata.bpm.max(20.0);
                        let pos = telemetry.as_ref().map(|tel| tel.deck_positions[i]).unwrap_or(0);
                        let loop_samples = (beats * (sample_rate * 60.0 / bpm)) as u64;
                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::SetLoop {
                            node_idx,
                            enabled: true,
                            start_samples: pos,
                            end_samples: pos + loop_samples,
                        }));
                    }
                }
            }

            let off_btn = egui::Button::new(RichText::new("OFF").size(theme.type_caption).strong())
                .min_size(Vec2::new(26.0, 20.0))
                .fill(theme.bg_surface);
            if ui.add(off_btn).clicked() {
                if let Some(node_idx) = node_idx {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::SetLoop {
                        node_idx,
                        enabled: false,
                        start_samples: 0,
                        end_samples: 0,
                    }));
                }
            }
        });
    });
}
