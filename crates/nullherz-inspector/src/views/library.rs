use egui::{Color32, RichText, Ui, ScrollArea, Layout, Align, Stroke, Frame, Margin, Rounding};
use crate::InspectorApp;
use nullherz_dna::GeneticLibrary;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.horizontal(|ui| {
        // 1. Crates Navigation Pane
        ui.vertical(|ui| {
            ui.set_max_width(90.0);
            ui.add_space(theme.space_xs);
            ui.label(RichText::new(format!("{} CRATES", egui_phosphor::regular::FOLDER)).size(theme.type_caption).strong().color(theme.text_secondary));
            ui.add_space(theme.space_sm);

            let is_all = app.active_crate.is_none();
            if ui.selectable_label(is_all, &format!("{} ALL", egui_phosphor::regular::PACKAGE)).clicked() { app.active_crate = None; }

            ui.add_space(theme.space_xs);
            let crates = app.library_db.list_crates().unwrap_or_default();
            for crate_name in crates {
                let is_selected = app.active_crate.as_deref() == Some(crate_name.as_str());
                if ui.selectable_label(is_selected, format!("{} {}", egui_phosphor::regular::TAG, crate_name)).clicked() {
                    app.active_crate = Some(crate_name);
                    app.library_needs_refresh = true;
                }
            }

            ui.add_space(theme.space_sm);
            ui.label(RichText::new(format!("{} SMART", egui_phosphor::regular::STAR)).size(theme.type_caption).strong().color(theme.text_secondary));
            let smart_crates = app.library_db.list_smart_crates().unwrap_or_default();
            for smart in smart_crates {
                let is_selected = app.active_crate.as_deref() == Some(smart.name.as_str());
                if ui.selectable_label(is_selected, &smart.name).clicked() {
                    app.active_crate = Some(smart.name);
                    app.library_needs_refresh = true;
                }
            }

            ui.add_space(theme.space_md);
            if ui.button(RichText::new("+ NEW").size(theme.type_caption)).clicked() {
                app.smart_crate_builder_open = !app.smart_crate_builder_open;
            }
        });

        ui.separator();

        // 2. Main Content Area
        ui.vertical(|ui| {
            // Smart Crate Builder
            if app.smart_crate_builder_open {
                render_smart_crate_builder(app, ui);
                ui.add_space(theme.space_sm);
            }

            // Library Toolbar
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE).on_hover_text("Refresh").clicked() { app.library_needs_refresh = true; }
                    ui.text_edit_singleline(&mut app.search_query);
                    ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);

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
            ui.add_space(theme.space_sm);

            // Track List
            render_track_list(app, ui);

            if let Some(track_id) = app.selected_library_track {
                ui.add_space(theme.space_sm);
                ui.separator();
                render_track_inspector(app, ui, track_id);
            }
        });
    });
}

fn render_smart_crate_builder(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    Frame::none()
        .fill(theme.bg_inset)
        .rounding(Rounding::same(theme.radius_md))
        .stroke(theme.border_stroke)
        .inner_margin(Margin::same(theme.space_md))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.strong("SMART CRATE BUILDER");
                ui.add_space(theme.space_xs);
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
    let theme = app.theme;
    if let Ok(Some(mut track)) = app.library_db.get_track(track_id) {
        Frame::none()
            .fill(theme.bg_inset)
            .rounding(Rounding::same(theme.radius_md))
            .stroke(theme.border_stroke)
            .inner_margin(Margin::same(theme.space_md))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut track.title);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button(egui_phosphor::regular::X).clicked() { app.selected_library_track = None; }
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

                    ui.add_space(theme.space_sm);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("GENETIC PROFILE").size(theme.type_caption).strong().color(theme.accent));
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
                        ui.add(egui::ProgressBar::new((track.metadata.dna.spectral.tilt + 1.0) / 2.0).fill(theme.deck_colors[1]));
                        ui.end_row();

                        ui.label("Syncopation:");
                        ui.add(egui::ProgressBar::new(track.metadata.dna.rhythmic.syncopation_index).fill(theme.success));
                        ui.end_row();

                        ui.label("Glitch Density:");
                        ui.add(egui::ProgressBar::new(track.metadata.dna.artifacts.glitch_density).fill(theme.deck_colors[2]));
                        ui.end_row();
                    });
                });
            });
    }
}

fn render_track_list(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
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
                ui.painter().rect_filled(rect, theme.radius_sm, Color32::from_white_alpha((hover_alpha * 15.0) as u8));
            }

            ui.child_ui(rect, Layout::left_to_right(Align::Center)).horizontal(|ui| {
                ui.add_space(theme.space_xs);
                let is_loaded = app.now_playing.iter().any(|np| np.as_ref() == Some(&track.id));
                let text_color = if is_loaded { theme.accent } else { theme.text_primary };

                ui.label(RichText::new(&track.title).color(text_color).strong().size(theme.type_caption));
                ui.label(RichText::new(&track.artist).color(theme.text_secondary).size(theme.type_caption));

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(egui_phosphor::regular::TRASH).on_hover_text("Delete track").clicked() {
                         let _ = app.library_db.remove_track(track.id);
                         app.library_needs_refresh = true;
                    }
                    ui.add_space(theme.space_xs);
                    ui.label(RichText::new(format!("{:.0}", track.metadata.bpm)).monospace().size(theme.type_caption).color(theme.text_secondary));

                    // SoundDNA Sparkline
                    let (spark_rect, _) = ui.allocate_at_least(egui::vec2(40.0, 10.0), egui::Sense::hover());
                    ui.painter().rect_filled(spark_rect, theme.radius_sm, theme.bg_inset);

                    let tilt = (track.metadata.dna.spectral.tilt + 1.0) / 2.0;
                    let sync = track.metadata.dna.rhythmic.syncopation_index;
                    let glitch = track.metadata.dna.artifacts.glitch_density;

                    let bar_w = spark_rect.width() / 3.0;
                    for (i, (val, color)) in [(tilt, theme.deck_colors[1]), (sync, theme.success), (glitch, theme.deck_colors[2])].iter().enumerate() {
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
                    let deck_char = (b'A' + deck_idx as u8) as char;
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::LoadTrackToDeck {
                        deck_id: deck_char,
                        sample_id: track.id,
                    }));
                    app.now_playing[deck_idx] = Some(track.id);
                }
            }

            ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, theme.border));
        }
    });
}
