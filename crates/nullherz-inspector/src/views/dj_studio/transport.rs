use egui::{Ui, RichText, Color32};
use crate::InspectorApp;

pub fn render_deck_transport(app: &mut InspectorApp, ui: &mut Ui, i: usize) {
    let deck_id = (b'A' + i as u8) as char;
    let node_name = match i {
        0 => "deck_a_sampler",
        1 => "deck_b_sampler",
        2 => "deck_c_sampler",
        3 => "deck_d_sampler",
        _ => "",
    };
    let node_idx = app.get_node_id(node_name);

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            // Play / Stop column
            ui.vertical(|ui| {
                // PLAY
                if ui.add_sized([36.0, 24.0], egui::Button::new(RichText::new("▶").size(12.0)).fill(Color32::from_gray(35))).clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::PlayDeck { deck_id }));
                }
                ui.add_space(4.0);
                // STOP (secondary small button under PLAY)
                if ui.add_sized([36.0, 16.0], egui::Button::new(RichText::new("⏸").size(10.0)).fill(Color32::from_gray(35))).clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::StopDeck { deck_id }));
                }
            });

            // CUE
            if ui.add_sized([36.0, 44.0], egui::Button::new(RichText::new("CUE").size(12.0).strong()).fill(Color32::from_gray(35))).clicked() {
                let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpToHotCue {
                    node_idx,
                    cue_idx: 0,
                }));
            }

            // SYNC
            if ui.add_sized([36.0, 44.0], egui::Button::new(RichText::new("SYNC").size(10.0).strong()).fill(Color32::from_rgb(0, 100, 150))).clicked() {
                let master_deck_id = (b'A' + app.master_deck.unwrap_or(0) as u8) as char;
                let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::SyncDecks {
                    source_deck: master_deck_id,
                    target_deck: deck_id,
                }));
            }
        });

        ui.add_space(6.0);

        // Professional Auto Loop Panel
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("LOOPING").small().color(Color32::from_gray(120)));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;

                // Loop Sizes (Common sizes 1, 4, 8 beats)
                for size in [1, 4, 8] {
                    let loop_lbl = format!("{}B", size);
                    if ui.add_sized([24.0, 18.0], egui::Button::new(RichText::new(loop_lbl).size(8.0))).clicked() {
                        // Loop from current playhead (sending typical SetLoop command)
                        let sample_rate = 44100.0;
                        let size_samples = (size as f32 * 60.0 / 120.0 * sample_rate) as u64; // base 120 bpm approx
                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::SetLoop {
                            node_idx,
                            enabled: true,
                            start_samples: 0,
                            end_samples: size_samples,
                        }));
                    }
                }

                // Dedicated Exit/Reloop button
                if ui.add_sized([30.0, 18.0], egui::Button::new(RichText::new("EXIT").size(8.0)).fill(Color32::from_rgb(100, 0, 0))).clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::SetLoop {
                        node_idx,
                        enabled: false,
                        start_samples: 0,
                        end_samples: 0,
                    }));
                }
            });
        });
    });
}
