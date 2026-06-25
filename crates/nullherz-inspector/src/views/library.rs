use egui::{Color32, RichText, Ui, Frame, ScrollArea, Layout, Align, Stroke};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    Frame::none().fill(Color32::from_rgb(12, 12, 14)).inner_margin(12.0).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("LIBRARY").color(Color32::from_gray(150)).small().strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("REFRESH").clicked() {
                    if let Ok(db) = nullherz_dna::LibraryDatabase::load("library.redb") {
                        app.library_db = db;
                    }
                }
            });
        });
        ui.add_space(10.0);
        ui.text_edit_singleline(&mut app.search_query);
        ui.add_space(15.0);

        ScrollArea::vertical().show(ui, |ui| {
            let mut tracks = app.library_db.list_tracks().unwrap_or_default();
            tracks.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));

            for track in &tracks {
                let title = &track.title;
                let artist = &track.artist;
                let bpm = track.metadata.bpm;
                let key = track.metadata.root_key.unwrap_or(0.0);

                if !app.search_query.is_empty() {
                    let q = app.search_query.to_lowercase();
                    if !title.to_lowercase().contains(&q) && !artist.to_lowercase().contains(&q) {
                        continue;
                    }
                }

                let (rect, res) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::click());
                let how_h = ui.ctx().animate_bool(res.id, res.hovered());
                if how_h > 0.0 { ui.painter().rect_filled(rect, 0.0, Color32::from_gray((how_h * 20.0) as u8)); }

                res.context_menu(|ui| {
                    for deck_idx in 0..4 {
                        if ui.button(format!("Load to Deck {}", (b'A' + deck_idx as u8) as char)).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::AddSourceFromRegistry {
                                granular_node_idx: (deck_idx as u32 * 4),
                                sample_id: track.id,
                            });
                                    app.now_playing[deck_idx] = Some(title.to_string());
                            ui.close_menu();
                        }
                    }
                });

                if res.clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::AddSourceFromRegistry {
                        granular_node_idx: (app.selected_deck as u32 * 4),
                        sample_id: track.id,
                    });
                            app.now_playing[app.selected_deck] = Some(title.to_string());
                }

                ui.child_ui(rect, Layout::left_to_right(Align::Center)).horizontal(|ui| {
                    ui.add_space(5.0);
                    let is_loaded = app.now_playing.iter().any(|np| np.as_deref() == Some(title));
                    let t_color = if is_loaded { Color32::from_rgb(0, 255, 150) } else { Color32::WHITE };

                    ui.label(RichText::new(format!("{} - {}", title, artist)).size(11.0).color(t_color));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(5.0);
                        ui.label(RichText::new(format!("{:.0}", bpm)).color(Color32::from_gray(80)).size(10.0));
                        ui.add_space(10.0);
                        ui.label(RichText::new(format!("K:{:.0}", key)).color(Color32::from_gray(60)).size(9.0));
                    });
                });
                ui.painter().hline(rect.x_range(), rect.max.y, Stroke::new(1.0, Color32::from_gray(20)));
            }
        });
    });
}
