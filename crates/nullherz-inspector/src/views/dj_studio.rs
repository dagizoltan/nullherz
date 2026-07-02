use egui::{Ui, Color32, RichText};
use crate::{InspectorApp, widgets};
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        ui.heading("DJ Precision Console");
        ui.add_space(20.0);
        ui.label(RichText::new("HARDENED SIGNAL PATH").color(Color32::from_gray(100)));
    });

    ui.add_space(10.0);

    ui.columns(2, |cols| {
        for i in 0..2 {
            let deck_color = InspectorApp::deck_color(i);
            cols[i].group(|ui| {
                ui.label(RichText::new(format!("DECK {}", (i + 65) as u8 as char)).strong().color(deck_color));
                ui.add_space(5.0);

                if ui.button("PLAY").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play));
                }

                if let Some(t) = telemetry {
                     widgets::render_vu_meter(ui, t.peak_levels[i], 1.0, deck_color, 150.0);
                }
            });
        }
    });
}
