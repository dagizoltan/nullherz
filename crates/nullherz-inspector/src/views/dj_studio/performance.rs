use egui::{Ui, RichText, Color32, Vec2};
use crate::InspectorApp;

pub fn render_deck_performance(app: &mut InspectorApp, ui: &mut Ui, i: usize) {
    ui.vertical(|ui| {
        ui.label(RichText::new("HOT-CUES").small().color(Color32::from_gray(100)));
        egui::Grid::new(format!("perf_grid_{}", i)).spacing([4.0, 4.0]).show(ui, |ui| {
            for row in 0..2 {
                for col in 0..4 {
                    let j = row * 4 + col;
                    let btn = egui::Button::new(RichText::new(format!("{}", j + 1)).strong())
                        .min_size(Vec2::new(32.0, 28.0))
                        .fill(Color32::from_gray(40));

                    if ui.add(btn).clicked() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpToHotCue {
                            node_idx: (i as u32 * 4),
                            cue_idx: j as u32,
                        }));
                    }
                }
                ui.end_row();
            }
        });
    });
}
