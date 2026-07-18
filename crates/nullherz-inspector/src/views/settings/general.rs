use egui::{Ui, Frame};
use crate::InspectorApp;

pub fn render_general(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("Global Transport");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Global BPM:");
                if ui.add(egui::DragValue::new(&mut app.decks.global_bpm).clamp_range(40.0..=300.0)).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetBpm(app.decks.global_bpm)));
                }
            });
            ui.add_space(theme.space_sm);
            if ui.checkbox(&mut app.mixer.quantize_enabled, "Quantize Commands").changed() {
                 let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetSafeMode(app.mixer.quantize_enabled)));
            }
        });
}
