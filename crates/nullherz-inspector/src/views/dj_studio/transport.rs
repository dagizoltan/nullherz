use egui::{Ui, RichText};
use crate::InspectorApp;

pub fn render_deck_transport(app: &mut InspectorApp, ui: &mut Ui, i: usize) {
    let deck_id = (b'A' + i as u8) as char;
    let theme = app.theme;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.space_xs;

        // Play / Stop column
        ui.vertical(|ui| {
            // PLAY
            if ui.add_sized([36.0, 24.0], egui::Button::new(RichText::new("▶").size(theme.type_caption)).fill(theme.bg_surface)).clicked() {
                let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::PlayDeck { deck_id }));
            }
            ui.add_space(theme.space_xs);
            // STOP (secondary small button under PLAY)
            if ui.add_sized([36.0, 16.0], egui::Button::new(RichText::new("⏸").size(theme.type_caption)).fill(theme.bg_surface)).clicked() {
                let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::StopDeck { deck_id }));
            }
        });

        // CUE
        if ui.add_sized([36.0, 44.0], egui::Button::new(RichText::new("CUE").size(theme.type_caption).strong()).fill(theme.bg_surface)).clicked() {
            let node_name = match i {
                0 => "deck_a_sampler",
                1 => "deck_b_sampler",
                2 => "deck_c_sampler",
                3 => "deck_d_sampler",
                _ => "",
            };
            let node_idx = app.get_node_id(node_name);
            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpToHotCue {
                node_idx,
                cue_idx: 0,
            }));
        }

        // SYNC
        if ui.add_sized([36.0, 44.0], egui::Button::new(RichText::new("SYNC").size(theme.type_caption).strong()).fill(theme.accent_muted)).clicked() {
            let master_deck_id = (b'A' + app.decks.master_deck.unwrap_or(0) as u8) as char;
            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::SyncDecks {
                source_deck: master_deck_id,
                target_deck: deck_id,
            }));
        }
    });
}
