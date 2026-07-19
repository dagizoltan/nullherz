use egui::{Color32, RichText, Ui, ScrollArea, Layout, Align, Stroke, Frame, Margin, Rounding, Sense};
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

            let is_all = app.library.active_crate.is_none();
            if ui.selectable_label(is_all, format!("{} ALL", egui_phosphor::regular::PACKAGE)).clicked() { app.library.active_crate = None; }

            ui.add_space(theme.space_xs);
            let crates = app.library_db.list_crates().unwrap_or_default();
            for crate_name in crates {
                let is_selected = app.library.active_crate.as_deref() == Some(crate_name.as_str());
                if ui.selectable_label(is_selected, format!("{} {}", egui_phosphor::regular::TAG, crate_name)).clicked() {
                    app.library.active_crate = Some(crate_name);
                    app.library.library_needs_refresh = true;
                }
            }

            ui.add_space(theme.space_sm);
            ui.label(RichText::new(format!("{} SMART", egui_phosphor::regular::STAR)).size(theme.type_caption).strong().color(theme.text_secondary));
            let smart_crates = app.library_db.list_smart_crates().unwrap_or_default();
            for smart in smart_crates {
                let is_selected = app.library.active_crate.as_deref() == Some(smart.name.as_str());
                if ui.selectable_label(is_selected, &smart.name).clicked() {
                    app.library.active_crate = Some(smart.name);
                    app.library.library_needs_refresh = true;
                }
            }

            ui.add_space(theme.space_md);
            if ui.button(RichText::new("+ NEW").size(theme.type_caption)).clicked() {
                app.library.smart_crate_builder_open = !app.library.smart_crate_builder_open;
            }
        });

        // Consistent themed vertical 1px border divider instead of raw ui.separator()
        ui.add_space(theme.space_xs);
        let (line_rect, _) = ui.allocate_exact_size(egui::vec2(1.0, ui.available_height()), Sense::hover());
        ui.painter().rect_filled(line_rect, Rounding::ZERO, theme.border);
        ui.add_space(theme.space_xs);

        // 2. Main Content Area
        ui.vertical(|ui| {
            // Smart Crate Builder
            if app.library.smart_crate_builder_open {
                render_smart_crate_builder(app, ui);
                ui.add_space(theme.space_sm);
            }

            // Library Toolbar - Refactored to avoid severe squeeze on 280px widths
            ui.vertical(|ui| {
                // Row 1: Search query input + Magnifier icon + Refresh button
                ui.horizontal(|ui| {
                    ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
                    ui.add_space(theme.space_xs);
                    ui.text_edit_singleline(&mut app.library.search_query);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE).on_hover_text("Refresh").clicked() {
                            app.library.library_needs_refresh = true;
                        }
                    });
                });
                ui.add_space(theme.space_xs);

                // Row 2: Ingestion path text-field + SCAN button
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut app.library.ingestion_path);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("SCAN").clicked() {
                            let mut path_bytes = [0u8; 256];
                            let bytes = app.library.ingestion_path.as_bytes();
                            let len = bytes.len().min(256);
                            path_bytes[..len].copy_from_slice(&bytes[..len]);
                            let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::ScanFolder { path: path_bytes }));
                        }
                    });
                });
            });
            ui.add_space(theme.space_sm);

            // Height budget: the TRACK LIST owns every pixel the inspector
            // doesn't need. Without an explicit budget the scroll area
            // auto-shrinks and the inspector floats directly under the last
            // row — the panel showed a handful of tracks over dead space.
            let inspector_open = app.library.selected_library_track.is_some();
            let inspector_budget = if inspector_open { 264.0 } else { 0.0 };
            let list_height = (ui.available_height() - inspector_budget).max(96.0);

            // Track List
            render_track_list(app, ui, list_height);

            if let Some(track_id) = app.library.selected_library_track {
                ui.add_space(theme.space_sm);
                let (line_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter().rect_filled(line_rect, Rounding::ZERO, theme.border);
                ui.add_space(theme.space_sm);
                render_track_inspector(app, ui, track_id);
            }
        });
    });
}

fn render_smart_crate_builder(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    render_card_group(ui, "SMART CRATE BUILDER", &theme, |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut app.library.smart_crate_def.name);
            });

            ui.horizontal(|ui| {
                ui.label("Threshold:");
                ui.add(egui::Slider::new(&mut app.library.smart_crate_def.threshold, 0.0..=1.0).show_value(true));
            });

            if ui.button("SAVE CRATE").clicked() {
                let _ = app.library_db.save_smart_crate(&app.library.smart_crate_def);
                app.library.smart_crate_builder_open = false;
                app.library.library_needs_refresh = true;
            }
        });
    });
}

fn render_track_inspector(app: &mut InspectorApp, ui: &mut Ui, track_id: u64) {
    let theme = app.theme;
    if let Ok(Some(mut track)) = app.library_db.get_track(track_id) {
        render_card_group(ui, "TRACK INSPECTOR", &theme, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut track.title);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(egui_phosphor::regular::X).clicked() { app.library.selected_library_track = None; }
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
                    app.library.library_needs_refresh = true;
                }

                ui.add_space(theme.space_sm);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("GENETIC PROFILE").size(theme.type_caption).strong().color(theme.accent));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("▶ PREVIEW").clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::Preview { sample_id: track.id }));
                        }
                        if ui.button("⚡ ENERGY MATCH").on_hover_text("Generate smart crate with similar energy").clicked() {
                            let tracks = app.library.cached_library_raw.clone();
                            let new_crate = nullherz_dna::SmartCrateManager::generate_energy_matched_crate(&track, tracks, 0.7);
                            let _ = app.library_db.save_smart_crate(&new_crate);
                            app.trigger_library_refresh();
                        }
                    });
                });

                egui::Grid::new("dna_inspector_grid").num_columns(2).spacing([theme.space_md, theme.space_xs]).show(ui, |ui| {
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

/// Compact row height: ~27 rows visible in a 700px sidebar.
const TRACK_ROW_H: f32 = 26.0;

fn render_track_list(app: &mut InspectorApp, ui: &mut Ui, list_height: f32) {
    let theme = app.theme;
    if app.library.library_needs_refresh
        && app.library.bg_library_loader.is_none() {
            app.trigger_library_refresh();
        }

    // Apply client-side search query filtering on top of cached_library
    let mut displayed_tracks = app.library.cached_library.clone();
    if !app.library.search_query.is_empty() {
        let q = app.library.search_query.to_lowercase();
        displayed_tracks.retain(|t| t.title.to_lowercase().contains(&q) || t.artist.to_lowercase().contains(&q));
    }

    ui.label(
        RichText::new(format!("{} TRACKS", displayed_tracks.len()))
            .size(theme.type_caption)
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    // Virtualized (show_rows): only visible rows are laid out, so a large
    // library scrolls at full frame rate. auto_shrink off: the list OWNS its
    // full budget even when short — that is what fills the sidebar.
    ScrollArea::vertical()
        .id_source("lib_scroll")
        .max_height(list_height)
        .min_scrolled_height(list_height)
        .auto_shrink([false, false])
        .show_rows(ui, TRACK_ROW_H, displayed_tracks.len(), |ui, row_range| {
        for track in &displayed_tracks[row_range] {
            let (rect, res) = ui.allocate_exact_size(egui::vec2(ui.available_width(), TRACK_ROW_H), egui::Sense::click());

            // Hover effect
            let hover_alpha = ui.ctx().animate_bool(res.id, res.hovered());
            if hover_alpha > 0.0 {
                ui.painter().rect_filled(rect, theme.radius_sm, Color32::from_white_alpha((hover_alpha * 15.0) as u8));
            }

            ui.child_ui(rect, Layout::left_to_right(Align::Center)).horizontal(|ui| {
                ui.add_space(theme.space_xs);
                let is_loaded = app.decks.now_playing.iter().any(|np| np.as_ref() == Some(&track.id));
                let text_color = if is_loaded { theme.accent } else { theme.text_primary };

                // Robust text truncation to completely prevent right-side control overlap
                let left_budget = (rect.width() - 105.0).max(10.0);
                ui.allocate_ui(egui::vec2(left_budget, rect.height()), |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(&track.title).color(text_color).strong().size(theme.type_caption)).truncate(true));
                        ui.add(egui::Label::new(RichText::new(&track.artist).color(theme.text_secondary).size(theme.type_caption)).truncate(true));
                    });
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(egui_phosphor::regular::TRASH).on_hover_text("Delete track").clicked() {
                         let _ = app.library_db.remove_track(track.id);
                         app.library.library_needs_refresh = true;
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
                app.library.selected_library_track = Some(track.id);
            }

            if res.double_clicked() {
                let deck_idx = app.decks.focused_deck;
                if deck_idx < 4 {
                    let deck_char = (b'A' + deck_idx as u8) as char;
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::LoadTrackToDeck {
                        deck_id: deck_char,
                        sample_id: track.id,
                    }));
                    app.decks.now_playing[deck_idx] = Some(track.id);
                }
            }

            ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, theme.border));
        }
    });
}

fn render_card_group<F>(ui: &mut Ui, title: &str, theme: &nullherz_ui_hal::Theme, add_contents: F)
where F: FnOnce(&mut Ui)
{
    ui.label(RichText::new(title).small().strong().color(theme.text_secondary));
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .stroke(theme.border_stroke)
        .inner_margin(Margin::same(theme.space_md))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}
