use egui::{Color32, RichText, Ui, ScrollArea, Layout, Align, Stroke};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        // Crate Sidebar (Mini)
        ui.vertical(|ui| {
            ui.set_max_width(80.0);
            ui.label(RichText::new("CRATES").small().strong());
            ui.separator();

            let is_all = app.active_crate.is_none();
            if ui.selectable_label(is_all, "ALL").clicked() { app.active_crate = None; }

            let crates = app.library_db.list_crates().unwrap_or_default();
            for crate_name in crates {
                let is_selected = app.active_crate.as_deref() == Some(&crate_name);
                if ui.selectable_label(is_selected, &crate_name).clicked() {
                    app.active_crate = Some(crate_name);
                    app.library_needs_refresh = true;
                }
            }

            let smart_crates = app.library_db.list_smart_crates().unwrap_or_default();
            for smart in smart_crates {
                let is_selected = app.active_crate.as_deref() == Some(&smart.name);
                if ui.selectable_label(is_selected, format!("✨ {}", smart.name)).clicked() {
                    app.active_crate = Some(smart.name);
                    app.library_needs_refresh = true;
                }
            }

            ui.add_space(20.0);
            if ui.button(RichText::new("+ SMART").small()).clicked() {
                app.smart_crate_builder_open = !app.smart_crate_builder_open;
            }
        });

        ui.separator();

        ui.vertical(|ui| {
            if app.smart_crate_builder_open {
                ui.group(|ui| {
                    ui.label(RichText::new("🛠 SMART CRATE BUILDER").strong());
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut app.smart_crate_def.name);
                    });

                    ui.horizontal(|ui| {
                        ui.label("Spectral Tilt:");
                        let mut min = app.smart_crate_def.spectral_tilt_range.map(|r| r.0).unwrap_or(-1.0);
                        let mut max = app.smart_crate_def.spectral_tilt_range.map(|r| r.1).unwrap_or(1.0);
                        ui.add(egui::Slider::new(&mut min, -1.0..=1.0).text("MIN"));
                        ui.add(egui::Slider::new(&mut max, -1.0..=1.0).text("MAX"));
                        app.smart_crate_def.spectral_tilt_range = Some((min, max));
                    });

                    ui.horizontal(|ui| {
                        ui.label("Syncopation:");
                        let mut min = app.smart_crate_def.rhythmic_syncopation_range.map(|r| r.0).unwrap_or(0.0);
                        let mut max = app.smart_crate_def.rhythmic_syncopation_range.map(|r| r.1).unwrap_or(1.0);
                        ui.add(egui::Slider::new(&mut min, 0.0..=1.0).text("MIN"));
                        ui.add(egui::Slider::new(&mut max, 0.0..=1.0).text("MAX"));
                        app.smart_crate_def.rhythmic_syncopation_range = Some((min, max));
                    });

                    if ui.button("SAVE SMART CRATE").clicked() {
                        let _ = app.library_db.save_smart_crate(&app.smart_crate_def);
                        app.smart_crate_builder_open = false;
                        app.library_needs_refresh = true;
                    }
                });
                ui.add_space(10.0);
            }

        // Change Tracking
        let breeding_a = app.breeding_view.parent_a_id;
        let breeding_b = app.breeding_view.parent_b_id;

        // Breeding Lab Header
        if breeding_a.is_some() || breeding_b.is_some() {
            ui.group(|ui| {
                ui.label(RichText::new("🧬 DNA BREEDING LAB").strong().color(Color32::from_rgb(0, 255, 200)));
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(format!("PARENT A: {}", app.breeding_view.parent_a_id.map(|id| id.to_string()).unwrap_or("NONE".to_string())));
                        if ui.button("CLEAR").clicked() { app.breeding_view.parent_a_id = None; }
                    });
                    ui.add_space(20.0);
                    ui.vertical(|ui| {
                        ui.label(format!("PARENT B: {}", app.breeding_view.parent_b_id.map(|id| id.to_string()).unwrap_or("NONE".to_string())));
                        if ui.button("CLEAR").clicked() { app.breeding_view.parent_b_id = None; }
                    });
                });

                if let (Some(_a), Some(_b)) = (app.breeding_view.parent_a_id, app.breeding_view.parent_b_id) {
                    ui.add_space(10.0);
                    ui.add(egui::Slider::new(&mut app.breeding_view.transfusion_bias_x, 0.0..=1.0).text("BIAS (A <-> B)"));
                    if ui.add(egui::Button::new("BREED CHILD DNA").fill(Color32::from_rgb(0, 100, 80))).clicked() {
                        // The actual breeding logic and registration would go here
                        // For now we simulate the action.
                    }
                }
            });
            ui.add_space(10.0);
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new("LIBRARY").color(Color32::from_gray(150)).small().strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("REFRESH").clicked() {
                    app.library_needs_refresh = true;
                }
            });
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.text_edit_singleline(&mut app.search_query).changed() {
                app.library_needs_refresh = true;
            }
            if ui.button("🔍").clicked() { app.library_needs_refresh = true; }
        });
        ui.add_space(15.0);

        // Optimized Library Retrieval & Sorting
        if app.library_needs_refresh {
            let mut tracks = if let Some(ref crate_name) = app.active_crate {
                // Try smart crate first
                if let Ok(smart_tracks) = app.library_db.get_smart_crate_tracks(crate_name) {
                    if !smart_tracks.is_empty() || app.library_db.get_smart_crate(crate_name).ok().flatten().is_some() {
                        smart_tracks
                    } else {
                        app.library_db.get_tracks_in_crate(crate_name).unwrap_or_default()
                    }
                } else {
                    app.library_db.get_tracks_in_crate(crate_name).unwrap_or_default()
                }
            } else {
                app.library_db.list_tracks().unwrap_or_default()
            };

            // Search filter
            if !app.search_query.is_empty() {
                let q = app.search_query.to_lowercase();
                tracks.retain(|t| t.title.to_lowercase().contains(&q) || t.artist.to_lowercase().contains(&q));
            }

            // Matchmaking Sort
            let breeding_target = if let Some(id_a) = breeding_a {
                app.library_db.get_track(id_a).ok().flatten().map(|t| t.metadata.dna)
            } else if let Some(id_b) = breeding_b {
                app.library_db.get_track(id_b).ok().flatten().map(|t| t.metadata.dna)
            } else {
                None
            };

            if let Some(target_dna) = breeding_target {
                tracks.sort_by(|a, b| {
                    let score_a = nullherz_dna::calculate_similarity(&target_dna, &a.metadata.dna);
                    let score_b = nullherz_dna::calculate_similarity(&target_dna, &b.metadata.dna);
                    score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
                });
            } else {
                tracks.sort_by_key(|a| a.title.to_lowercase());
            }

            app.cached_library = tracks;
            app.library_needs_refresh = false;
        }

        ScrollArea::vertical().show(ui, |ui| {
            let tracks = &app.cached_library;

            for track in tracks {
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
                    if ui.button("Set as Breeding Parent A").clicked() {
                        app.breeding_view.parent_a_id = Some(track.id);
                        app.library_needs_refresh = true;
                        ui.close_menu();
                    }
                    if ui.button("Set as Breeding Parent B").clicked() {
                        app.breeding_view.parent_b_id = Some(track.id);
                        app.library_needs_refresh = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    for deck_idx in 0..4 {
                        if ui.button(format!("Load to Deck {}", (b'A' + deck_idx as u8) as char)).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::AddSourceFromRegistry {
                                granular_node_idx: (deck_idx as u32 * 4),
                                sample_id: track.id,
                            }));
                            app.now_playing[deck_idx] = Some(title.to_string());
                            ui.close_menu();
                        }
                    }
                });

                if res.clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::AddSourceFromRegistry {
                        granular_node_idx: (app.selected_deck as u32 * 4),
                        sample_id: track.id,
                    }));
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
    });
}
