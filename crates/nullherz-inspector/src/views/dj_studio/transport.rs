use egui::{Ui, RichText, Color32};
use crate::InspectorApp;

pub fn render_deck_transport(app: &mut InspectorApp, ui: &mut Ui, i: usize) {
    ui.vertical(|ui| {
        ui.set_min_width(50.0);
        let deck_id = (b'A' + i as u8) as char;
        if ui.add_sized([45.0, 40.0], egui::Button::new(RichText::new("▶").size(18.0)).fill(Color32::from_gray(35))).clicked() {
            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::PlayDeck { deck_id }));
        }
        ui.add_space(6.0);
        if ui.add_sized([45.0, 40.0], egui::Button::new(RichText::new("⏸").size(18.0)).fill(Color32::from_gray(35))).clicked() {
            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::StopDeck { deck_id }));
        }
    });
}
