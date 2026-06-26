use egui::{Ui, RichText, Frame, Color32, Layout, Align, ScrollArea};
use crate::{InspectorApp, Track, Playlist};

const PANEL_ROUNDING: f32 = 4.0;
const INNER_MARGIN: f32 = 8.0;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("Precision Hi-Fi Player");
    ui.add_space(INNER_MARGIN);

    ui.horizontal_top(|ui| {
        // Left: Library & Playlists
        ui.vertical(|ui| {
            ui.set_width(320.0);

            ui.strong("Collections");
            ui.add_space(4.0);
            for (idx, pl) in app.playlists.iter().enumerate() {
                let is_sel = app.selected_playlist == Some(idx);
                if ui.selectable_label(is_sel, format!("📁 {}", pl.name)).clicked() {
                    app.selected_playlist = Some(idx);
                }
            }
            if ui.button("+ New Playlist").clicked() {
                app.playlists.push(Playlist { name: format!("Playlist {}", app.playlists.len() + 1), tracks: vec![] });
            }

            ui.add_space(20.0);
            ui.strong("Quick Access Library");
            ui.separator();
            ScrollArea::vertical().id_source("player_lib").max_height(300.0).show(ui, |ui| {
                if let Ok(tracks) = app.library_db.list_tracks() {
                    for track in tracks {
                        ui.horizontal(|ui| {
                            if ui.button("➕").on_hover_text("Add to selected playlist").clicked() {
                                if let Some(idx) = app.selected_playlist {
                                    app.playlists[idx].tracks.push(Track { title: track.title.clone(), artist: track.artist.clone(), bpm: track.metadata.bpm as f32 });
                                }
                            }
                            ui.label(format!("{} - {}", track.artist, track.title));
                        });
                    }
                }
            });
        });

        ui.add_space(20.0);

        // Right: Content Area
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 20.0);
            if let Some(idx) = app.selected_playlist {
                let pl = &mut app.playlists[idx];
                ui.horizontal(|ui| {
                    ui.heading(&pl.name);
                    ui.add_space(10.0);
                    ui.label(RichText::new(format!("{} tracks", pl.tracks.len())).weak());
                });
                ui.separator();
                ui.add_space(10.0);

                ScrollArea::vertical().id_source("playlist_tracks").show(ui, |ui| {
                    let mut to_remove = None;
                    for (t_idx, trk) in pl.tracks.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.button("▶").clicked() {
                                app.player_is_playing = true;
                            }
                            ui.label(RichText::new(&trk.artist).strong());
                            ui.label("-");
                            ui.label(&trk.title);

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("🗑").clicked() { to_remove = Some(t_idx); }
                                ui.label(RichText::new(format!("{:.0} BPM", trk.bpm)).weak());
                            });
                        });
                        ui.add_space(2.0);
                    }
                    if let Some(idx) = to_remove { pl.tracks.remove(idx); }
                });
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.label(RichText::new("Select a Collection to begin listening").size(18.0).weak());
                });
            }
        });
    });

    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
        ui.add_space(INNER_MARGIN);
        Frame::none().fill(Color32::from_rgb(20, 20, 24)).inner_margin(INNER_MARGIN).rounding(PANEL_ROUNDING).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
               let _ = ui.button(RichText::new("⏮").size(20.0));
               if ui.button(RichText::new(if app.player_is_playing { "⏸" } else { "▶" }).size(24.0)).clicked() {
                   app.player_is_playing = !app.player_is_playing;
               }
               let _ = ui.button(RichText::new("⏭").size(20.0));

               ui.add_space(20.0);
               ui.vertical(|ui| {
                   ui.label("Now Playing: -");
                   ui.spacing_mut().slider_width = ui.available_width() - 100.0;
                   ui.add(egui::Slider::new(&mut 0.0, 0.0..=1.0).show_value(false));
               });
            });
        });
    });
}
