use egui::{Ui, RichText, Color32, Vec2};
use crate::InspectorApp;

pub fn render_deck_performance(app: &mut InspectorApp, ui: &mut Ui, i: usize) {
    ui.vertical(|ui| {
        ui.label(RichText::new("PERFORM").small().color(Color32::from_gray(100)));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(2.0);
            for j in 0..8 {
                if ui.add_sized([28.0, 24.0], egui::Button::new(format!("{}", j + 1)).fill(Color32::from_gray(30))).clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpToHotCue {
                        node_idx: (i as u32 * 4),
                        cue_idx: j as u32,
                    }));
                }
            }
        });
    });
}
