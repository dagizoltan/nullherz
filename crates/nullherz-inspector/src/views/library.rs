use egui::{Color32, RichText, Ui, ScrollArea, Layout, Align, Stroke, Frame, Margin};
use crate::InspectorApp;
use nullherz_dna::GeneticLibrary;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        // 1. Crates Navigation Pane
        ui.vertical(|ui| {
            ui.set_max_width(90.0);
            ui.add_space(5.0);
            ui.label(RichText::new("📁 CRATES").small().strong().color(Color32::from_gray(120)));
            ui.add_space(8.0);

            let is_all = app.active_crate.is_none();
            if ui.selectable_label(is_all, "📦 ALL").clicked() { app.active_crate = None; }

            ui.add_space(4.0);
            let crates = app.library_db.list_crates().unwrap_or_default();
            for crate_name in crates {
                let is_selected = app.active_crate.as_deref() == Some(crate_name.as_str());
                if ui.selectable_label(is_selected, format!("🏷 {}", crate_name)).clicked() {
                    app.active_crate = Some(crate_name);
                    app.library_needs_refresh = true;
                }
            }

            ui.add_space(8.0);
            ui.label(RichText::new("✨ SMART").small().strong().color(Color32::from_gray(120)));
            let smart_crates = app.library_db.list_smart_crates().unwrap_or_default();
            for smart in smart_crates {
                let is_selected = app.active_crate.as_deref() == Some(smart.name.as_str());
                if ui.selectable_label(is_selected, &smart.name).clicked() {
                    app.active_crate = Some(smart.name);
                    app.library_needs_refresh = true;
                }
            }

            ui.add_space(20.0);
            if ui.button(RichText::new("+ NEW").small()).clicked() {
                app.smart_crate_builder_open = !app.smart_crate_builder_open;
            }
        });

        ui.separator();

        // 2. Main Content Area
        ui.vertical(|ui| {
            // Smart Crate Builder
            if app.smart_crate_builder_open {
                render_smart_crate_builder(app, ui);
                ui.add_space(10.0);
            }

            // Library Toolbar
            ui.horizontal(|ui| {
                ui.label(RichText::new("LIBRARY").strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("🔄").on_hover_text("Refresh").clicked() { app.library_needs_refresh = true; }
                    ui.text_edit_singleline(&mut app.search_query);
                    ui.label("🔍");

                    if ui.button("SCAN").clicked() {
                        let mut path_bytes = [0u8; 256];
                        let bytes = app.ingestion_path.as_bytes();
                        let len = bytes.len().min(256);
                        path_bytes[..len].copy_from_slice(&bytes[..len]);
                        let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::ScanFolder { path: path_bytes }));
                    }
                    ui.text_edit_singleline(&mut app.ingestion_path);
                });
            });
            ui.add_space(10.0);

            // Track List
            render_track_list(app, ui);

            if let Some(track_id) = app.selected_library_track {
                ui.add_space(10.0);
                ui.separator();
                render_track_inspector(app, ui, track_id);
            }
        });
    });
}

fn render_smart_crate_builder(app: &mut InspectorApp, ui: &mut Ui) {
    Frame::group(ui.style()).fill(Color32::from_rgb(20, 25, 30)).inner_margin(Margin::same(10.0)).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.strong("SMART CRATE BUILDER");
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut app.smart_crate_def.name);
            });

            ui.horizontal(|ui| {
                ui.label("Threshold:");
                ui.add(egui::Slider::new(&mut app.smart_crate_def.threshold, 0.0..=1.0).show_value(true));
            });

            if ui.button("SAVE CRATE").clicked() {
                let _ = app.library_db.save_smart_crate(&app.smart_crate_def);
                app.smart_crate_builder_open = false;
                app.library_needs_refresh = true;
            }
        });
    });
}

fn render_track_inspector(app: &mut InspectorApp, ui: &mut Ui, track_id: u64) {
    if let Ok(Some(mut track)) = app.library_db.get_track(track_id) {
        Frame::group(ui.style()).fill(Color32::from_rgb(15, 15, 20)).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut track.title);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("❌").clicked() { app.selected_library_track = None; }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Artist:");
                    ui.text_edit_singleline(&mut track.artist);
                });
                ui.horizontal(|ui| {
                    ui.label("Genre:");
                    ui.text_edit_singleline(&mut track.genre);
                });

                if ui.button("SAVE CHANGES").clicked() {
                    let _ = app.library_db.save_track(&track);
                    app.library_needs_refresh = true;
                }

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("GENETIC PROFILE").small().strong().color(app.theme.accent));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("▶ PREVIEW").clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::Preview { sample_id: track.id }));
                        }
                        if ui.button("⚡ ENERGY MATCH").on_hover_text("Generate smart crate with similar energy").clicked() {
                            let tracks = app.library_db.list_tracks().unwrap_or_default();
                            let new_crate = nullherz_dna::SmartCrateManager::generate_energy_matched_crate(&track, tracks, 0.7);
                            let _ = app.library_db.save_smart_crate(&new_crate);
                            app.library_needs_refresh = true;
                        }
                    });
                });

                egui::Grid::new("dna_inspector_grid").num_columns(2).spacing([20.0, 4.0]).show(ui, |ui| {
                    ui.label("Spectral Tilt:");
                    ui.add(egui::ProgressBar::new((track.metadata.dna.spectral.tilt + 1.0) / 2.0).fill(Color32::from_rgb(0, 200, 255)));
                    ui.end_row();

                    ui.label("Syncopation:");
                    ui.add(egui::ProgressBar::new(track.metadata.dna.rhythmic.syncopation_index).fill(Color32::from_rgb(0, 255, 150)));
                    ui.end_row();

                    ui.label("Glitch Density:");
                    ui.add(egui::ProgressBar::new(track.metadata.dna.artifacts.glitch_density).fill(Color32::from_rgb(255, 100, 0)));
                    ui.end_row();
                });
            });
        });
    }
}

fn render_track_list(app: &mut InspectorApp, ui: &mut Ui) {
    if app.library_needs_refresh {
        // Logic for fetching and sorting (reused from previous version)
        let mut tracks = if let Some(ref crate_name) = app.active_crate {
            app.library_db.get_tracks_in_crate(crate_name).unwrap_or_default()
        } else {
            app.library_db.list_tracks().unwrap_or_default()
        };

        if !app.search_query.is_empty() {
            let q = app.search_query.to_lowercase();
            tracks.retain(|t| t.title.to_lowercase().contains(&q) || t.artist.to_lowercase().contains(&q));
        }

        app.cached_library = tracks;
        app.library_needs_refresh = false;
    }

    ScrollArea::vertical().id_source("lib_scroll").show(ui, |ui| {
        for track in &app.cached_library {
            let (rect, res) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::click());

            // Hover effect
            let hover_alpha = ui.ctx().animate_bool(res.id, res.hovered());
            if hover_alpha > 0.0 {
                ui.painter().rect_filled(rect, 2.0, Color32::from_white_alpha((hover_alpha * 15.0) as u8));
            }

            ui.child_ui(rect, Layout::left_to_right(Align::Center)).horizontal(|ui| {
                ui.add_space(5.0);
                let is_loaded = app.now_playing.iter().any(|np| np.as_ref() == Some(&track.id));
                let text_color = if is_loaded { Color32::from_rgb(0, 255, 180) } else { Color32::WHITE };

                ui.label(RichText::new(&track.title).color(text_color).strong().size(11.0));
                ui.label(RichText::new(&track.artist).color(Color32::from_gray(120)).size(10.0));

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("🗑").on_hover_text("Delete track").clicked() {
                         let _ = app.library_db.remove_track(track.id);
                         app.library_needs_refresh = true;
                    }
                    ui.add_space(5.0);
                    ui.label(RichText::new(format!("{:.0}", track.metadata.bpm)).monospace().size(9.0).color(Color32::from_gray(120)));

                    // SoundDNA Sparkline
                    let (spark_rect, _) = ui.allocate_at_least(egui::vec2(40.0, 10.0), egui::Sense::hover());
                    ui.painter().rect_filled(spark_rect, 1.0, Color32::from_rgb(25, 25, 30));

                    let tilt = (track.metadata.dna.spectral.tilt + 1.0) / 2.0;
                    let sync = track.metadata.dna.rhythmic.syncopation_index;
                    let glitch = track.metadata.dna.artifacts.glitch_density;

                    let bar_w = spark_rect.width() / 3.0;
                    for (i, (val, color)) in [(tilt, Color32::from_rgb(0, 200, 255)), (sync, Color32::from_rgb(0, 255, 150)), (glitch, Color32::from_rgb(255, 100, 0))].iter().enumerate() {
                        let h = spark_rect.height() * val.clamp(0.1, 1.0);
                        let x = spark_rect.left() + (i as f32 * bar_w);
                        let r = egui::Rect::from_min_max(egui::pos2(x + 1.0, spark_rect.bottom() - h), egui::pos2(x + bar_w - 1.0, spark_rect.bottom()));
                        ui.painter().rect_filled(r, 0.5, *color);
                    }
                });
            });

            if res.clicked() {
                app.selected_library_track = Some(track.id);
            }

            if res.double_clicked() {
                let deck_idx = app.focused_deck;
                if deck_idx < 4 {
                    let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::AddSourceFromRegistry {
                        granular_node_idx: (deck_idx as u32 * 4),
                        sample_id: track.id,
                    }));
                    app.now_playing[deck_idx] = Some(track.id);
                }
            }

            ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, Color32::from_gray(25)));
        }
    });
}
