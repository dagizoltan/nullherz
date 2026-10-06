use egui::{Ui, Frame};
use crate::InspectorApp;

use egui::RichText;

pub fn render_general(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    ui.label(RichText::new("GLOBAL TRANSPORT & TIMING").small().strong().color(theme.text_secondary));
    ui.add_space(theme.space_xs);

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Global System Tempo (BPM):").strong().color(theme.text_primary));
                ui.add_space(theme.space_xs);
                if ui.add(egui::DragValue::new(&mut app.decks.global_bpm).range(40.0..=300.0).speed(0.1).suffix(" BPM")).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetBpm(app.decks.global_bpm)));
                }
            });
            ui.add_space(theme.space_sm);
            if ui.checkbox(&mut app.mixer.quantize_enabled, "Quantize Commands (Safe Mode)").on_hover_text("Lock performance triggers and hot-cue jumps to beat-grid boundaries").changed() {
                 let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetSafeMode(app.mixer.quantize_enabled)));
            }
        });
}
