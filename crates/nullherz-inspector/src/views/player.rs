use egui::{Ui, ScrollArea};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("Precision Player");
    ui.add_space(20.0);

    ui.horizontal(|ui| {
        if ui.button(if app.player_is_playing { "PAUSE" } else { "PLAY" }).clicked() {
            app.player_is_playing = !app.player_is_playing;
            if app.player_is_playing {
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play));
            } else {
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop));
            }
        }
    });

    ui.add_space(10.0);
    ScrollArea::vertical().show(ui, |ui| {
        for playlist in &app.playlists {
            ui.collapsing(&playlist.name, |ui| {
                for track in &playlist.tracks {
                    ui.label(format!("{} - {}", track.artist, track.title));
                }
            });
        }
    });
}
