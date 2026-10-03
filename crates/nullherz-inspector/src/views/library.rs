use egui::{Color32, RichText, Ui, ScrollArea, Layout, Align, Stroke, Frame, Margin, Rounding};
use crate::InspectorApp;
use nullherz_dna::GeneticLibrary;
use sidecar_sdk::AssetCategory;

#[derive(Clone, Debug)]
pub struct AudioLocation {
    pub label: String,
    pub path: String,
    pub is_external: bool,
}

pub fn detect_audio_locations() -> Vec<AudioLocation> {
    let mut locations = vec![
        AudioLocation { label: "Tracks Folder".to_string(), path: "library/tracks/".to_string(), is_external: false },
        AudioLocation { label: "Samples Folder".to_string(), path: "library/samples/".to_string(), is_external: false },
        AudioLocation { label: "Sequences Folder".to_string(), path: "library/sequences/".to_string(), is_external: false },
    ];

    if let Ok(user_music) = std::env::var("HOME") {
        let music_dir = format!("{}/Music", user_music);
        if std::path::Path::new(&music_dir).exists() {
            locations.push(AudioLocation {
                label: "User Music".to_string(),
                path: music_dir,
                is_external: false,
            });
        }
    }

    // Scan mount roots for pendrives / external drives (Linux / macOS / Unix)
    let mount_roots = ["/media", "/run/media", "/mnt", "/Volumes"];
    for root in &mount_roots {
        let p = std::path::Path::new(root);
        if p.exists() && p.is_dir() {
            if let Ok(entries) = std::fs::read_dir(p) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        // Check if user subdirectories exist under /media or /run/media
                        if root.contains("media") {
                            if let Ok(sub_entries) = std::fs::read_dir(&path) {
                                for sub in sub_entries.flatten() {
                                    let sub_path = sub.path();
                                    if sub_path.is_dir() {
                                        let name = sub.file_name().to_string_lossy().to_string();
                                        locations.push(AudioLocation {
                                            label: format!("External Drive ({})", name),
                                            path: sub_path.to_string_lossy().to_string(),
                                            is_external: true,
                                        });
                                    }
                                }
                            }
                        } else {
                            let name = entry.file_name().to_string_lossy().to_string();
                            locations.push(AudioLocation {
                                label: format!("External Drive ({})", name),
                                path: path.to_string_lossy().to_string(),
                                is_external: true,
                            });
                        }
                    }
                }
            }
        }
    }

    locations
}

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    ui.vertical(|ui| {
        // Crates Grid and Smart Crates Grid under each other
        render_crates_and_smart_crates_section(app, ui);

        ui.add_space(theme.space_sm);
        ui.separator();
        ui.add_space(theme.space_sm);

        // Track Browser (Toolbar + Tracks) under
        render_toolbar(app, ui);

        ui.add_space(theme.space_sm);

        render_asset_list(app, ui);
    });
}

fn render_crates_and_smart_crates_section(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    // 1. Categories Header
    ui.label(
        RichText::new("CATEGORIES")
            .size(theme.type_caption)
            .strong()
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    // Main Category Selector Chips
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(theme.space_xs, theme.space_xs);

        let is_all_cat = app.library.active_category.is_none() && app.library.active_crate.is_none();
        if ui.selectable_label(is_all_cat, format!("{} ALL", egui_phosphor::regular::PACKAGE)).clicked() {
            app.library.active_category = None;
            app.library.active_crate = None;
            app.library.library_needs_refresh = true;
        }

        for category in AssetCategory::all() {
            let is_selected = app.library.active_category == Some(*category) && app.library.active_crate.is_none();
            let icon = match category {
                AssetCategory::AudioFiles => egui_phosphor::regular::MUSIC_NOTES,
                AssetCategory::AudioInsert => egui_phosphor::regular::SLIDERS_HORIZONTAL,
                AssetCategory::VisualInsert => egui_phosphor::regular::EYE,
                AssetCategory::AudioInstrument => egui_phosphor::regular::PIANO_KEYS,
                AssetCategory::VisualInstrument => egui_phosphor::regular::APERTURE,
            };

            if ui.selectable_label(is_selected, format!("{} {}", icon, category.name().to_uppercase())).clicked() {
                app.library.active_category = Some(*category);
                app.library.active_crate = None;
                app.library.library_needs_refresh = true;
            }
        }
    });

    ui.add_space(theme.space_sm);

    // 2. User Crates & Smart Crates Header + NEW button
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{} USER & SMART CRATES", egui_phosphor::regular::FOLDER))
                .size(theme.type_caption)
                .strong()
                .color(theme.text_secondary),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(RichText::new("+ NEW SMART").size(theme.type_caption)).clicked() {
                app.library.smart_crate_builder_open = !app.library.smart_crate_builder_open;
            }
        });
    });
    ui.add_space(theme.space_xs);

    // User Crates & Smart Crates Wrapping Grid
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(theme.space_xs, theme.space_xs);

        let is_stems_cat = app.library.active_crate.as_deref() == Some("stems");
        if ui.selectable_label(is_stems_cat, format!("{} DEMIXED STEMS", egui_phosphor::regular::LIGHTNING)).clicked() {
            app.library.active_crate = Some("stems".to_string());
            app.library.active_category = None;
            app.library.library_needs_refresh = true;
        }

        let crates = &app.library.cached_crates;
        for crate_name in crates {
            if ["track", "sample", "sequence", "instrument", "insert", "visual", "stems"].contains(&crate_name.as_str()) {
                continue;
            }
            let is_selected = app.library.active_crate.as_deref() == Some(crate_name.as_str());
            if ui.selectable_label(is_selected, format!("{} {}", egui_phosphor::regular::TAG, crate_name)).clicked() {
                app.library.active_crate = Some(crate_name.clone());
                app.library.active_category = None;
                app.library.library_needs_refresh = true;
            }
        }

        let smart_crates = &app.library.cached_smart_crates;
        for smart in smart_crates {
            let is_selected = app.library.active_crate.as_deref() == Some(smart.name.as_str());
            if ui.selectable_label(is_selected, format!("{} {}", egui_phosphor::regular::STAR, smart.name)).clicked() {
                app.library.active_crate = Some(smart.name.clone());
                app.library.active_category = None;
                app.library.library_needs_refresh = true;
            }
        }
    });
}

fn render_toolbar(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    // Smart Crate Builder
    if app.library.smart_crate_builder_open {
        render_smart_crate_builder(app, ui);
        ui.add_space(theme.space_sm);
    }

    // Row 1: Search query input + Magnifier icon + Refresh button
    ui.horizontal(|ui| {
        ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
        ui.add_space(theme.space_xs);
        ui.text_edit_singleline(&mut app.library.search_query);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE).on_hover_text("Refresh").clicked() {
                app.library.library_needs_refresh = true;
            }
            egui::ComboBox::from_id_source("lib_sort")
                .selected_text(track_sort_label(app.library.sort))
                .show_ui(ui, |ui| {
                    use nullherz_dna::TrackSort::*;
                    for s in [Title, Artist, Album, Genre, BpmAsc, BpmDesc, EnergyAsc, EnergyDesc] {
                        ui.selectable_value(&mut app.library.sort, s, track_sort_label(s));
                    }
                })
                .response.on_hover_text("Sort");
        });
    });
    ui.add_space(theme.space_xs);

    // Row 2: Location selector + Ingestion path text-field + SCAN button
    ui.horizontal(|ui| {
        let locations = detect_audio_locations();
        let current_path = app.library.ingestion_path.clone();

        let selected_label = locations
            .iter()
            .find(|loc| loc.path == current_path)
            .map(|loc| loc.label.as_str())
            .unwrap_or("Custom Path");

        egui::ComboBox::from_id_source("lib_location_select")
            .selected_text(RichText::new(format!("{} {}", egui_phosphor::regular::HARD_DRIVES, selected_label)).size(theme.type_caption).strong())
            .width(160.0)
            .show_ui(ui, |ui| {
                for loc in &locations {
                    let prefix = if loc.is_external { "💾 " } else { "📁 " };
                    if ui.selectable_label(
                        current_path == loc.path,
                        RichText::new(format!("{}{}", prefix, loc.label)).size(theme.type_caption)
                    ).clicked() {
                        app.library.ingestion_path = loc.path.clone();
                        let mut path_bytes = [0u8; 256];
                        let bytes = loc.path.as_bytes();
                        let len = bytes.len().min(256);
                        path_bytes[..len].copy_from_slice(&bytes[..len]);
                        let _ = app.command_sender.send(nullherz_traits::Command::Resource(
                            nullherz_traits::ResourceCommand::ScanFolder { path: path_bytes }
                        ));
                        app.library.library_needs_refresh = true;
                    }
                }
            });

        ui.add_space(theme.space_xs);
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

fn track_sort_label(s: nullherz_dna::TrackSort) -> &'static str {
    use nullherz_dna::TrackSort::*;
    match s {
        Title => "Title",
        Artist => "Artist",
        Album => "Album",
        Genre => "Genre",
        BpmAsc => "BPM \u{2191}",
        BpmDesc => "BPM \u{2193}",
        EnergyAsc => "Energy \u{2191}",
        EnergyDesc => "Energy \u{2193}",
    }
}

/// Compact row height: ~27 rows visible in a 700px sidebar.
const TRACK_ROW_H: f32 = 26.0;
/// Height of the inline detail panel on an expanded row.
const TRACK_DETAIL_H: f32 = 250.0;

fn render_asset_list(app: &mut InspectorApp, ui: &mut Ui) {
    if app.library.library_needs_refresh
        && app.library.bg_library_loader.is_none() {
            app.trigger_library_refresh();
        }

    let search_q = app.library.search_query.trim().to_lowercase();

    if let Some(cat) = app.library.active_category {
        match cat {
            AssetCategory::AudioFiles => {
                render_audio_files_list(app, ui, &search_q);
            }
            AssetCategory::AudioInsert
            | AssetCategory::VisualInsert
            | AssetCategory::AudioInstrument
            | AssetCategory::VisualInstrument => {
                render_sidecars_for_category(app, ui, cat, &search_q);
            }
        }
    } else if app.library.active_crate.is_some() {
        render_audio_files_list(app, ui, &search_q);
    } else {
        render_all_categories_list(app, ui, &search_q);
    }
}

fn render_audio_files_list(app: &mut InspectorApp, ui: &mut Ui, search_q: &str) {
    let theme = app.theme;
    let mut displayed_tracks = app.library.cached_library.clone();

    if app.library.active_crate.as_deref() == Some("stems") {
        displayed_tracks.retain(|t| t.stems.is_some());
    }

    if !search_q.is_empty() {
        displayed_tracks.retain(|t| {
            t.title.to_lowercase().contains(search_q)
                || t.artist.to_lowercase().contains(search_q)
                || t.album.to_lowercase().contains(search_q)
                || t.genre.to_lowercase().contains(search_q)
        });
    }
    app.library.sort.order_tracks(&mut displayed_tracks);

    ui.label(
        RichText::new(format!("{} AUDIO FILES", displayed_tracks.len()))
            .size(theme.type_caption)
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ScrollArea::vertical()
        .id_source("lib_scroll")
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let expanded = app.library.expanded_track;
            let row_h = |t: &nullherz_dna::LibraryTrack| -> f32 {
                if expanded == Some(t.id) {
                    let stem_count = t.stems.as_ref().map(|s| s.stems.len()).unwrap_or(0);
                    let stem_h = (stem_count as f32 * 48.0).min(200.0);
                    TRACK_ROW_H + TRACK_DETAIL_H + stem_h
                } else {
                    TRACK_ROW_H
                }
            };

            let mut first = 0usize;
            let mut skipped_h = 0.0f32;
            let mut y = 0.0f32;
            for (idx, t) in displayed_tracks.iter().enumerate() {
                let h = row_h(t);
                if y + h >= viewport.min.y { first = idx; skipped_h = y; break; }
                y += h;
                first = idx + 1;
                skipped_h = y;
            }
            let mut last = first;
            let mut visible_h = 0.0f32;
            while last < displayed_tracks.len() && skipped_h + visible_h < viewport.max.y {
                visible_h += row_h(&displayed_tracks[last]);
                last += 1;
            }
            let after_h: f32 = displayed_tracks[last..].iter().map(row_h).sum();

            ui.add_space(skipped_h);
            for track in &displayed_tracks[first..last] {
                render_track_row(app, ui, track);
            }
            ui.add_space(after_h);
        });
}

fn render_sidecars_for_category(
    app: &mut InspectorApp,
    ui: &mut Ui,
    category: AssetCategory,
    search_q: &str,
) {
    let theme = app.theme;
    let mut descriptors = app.store.store_catalog.filter_by_category(category);
    if !search_q.is_empty() {
        descriptors.retain(|d| {
            d.id.to_lowercase().contains(search_q)
                || d.name.to_lowercase().contains(search_q)
                || d.description.to_lowercase().contains(search_q)
                || d.tags.iter().any(|t| t.to_lowercase().contains(search_q))
        });
    }

    ui.label(
        RichText::new(format!("{} MODULES IN {}", descriptors.len(), category.name().to_uppercase()))
            .size(theme.type_caption)
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ScrollArea::vertical()
        .id_source("lib_sidecar_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for descriptor in descriptors {
                render_sidecar_card_in_library(app, ui, &descriptor);
                ui.add_space(theme.space_sm);
            }
        });
}

fn render_all_categories_list(app: &mut InspectorApp, ui: &mut Ui, search_q: &str) {
    let theme = app.theme;

    let mut displayed_tracks = app.library.cached_library.clone();
    if !search_q.is_empty() {
        displayed_tracks.retain(|t| {
            t.title.to_lowercase().contains(search_q)
                || t.artist.to_lowercase().contains(search_q)
                || t.album.to_lowercase().contains(search_q)
                || t.genre.to_lowercase().contains(search_q)
        });
    }
    app.library.sort.order_tracks(&mut displayed_tracks);

    let store_list = app.store.store_catalog.list();

    ScrollArea::vertical()
        .id_source("lib_all_categories_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Section 1: Audio Files
            ui.label(
                RichText::new(format!("━━━ AUDIO FILES ({}) ━━━", displayed_tracks.len()))
                    .size(theme.type_caption)
                    .strong()
                    .color(theme.accent),
            );
            ui.add_space(theme.space_xs);

            for track in &displayed_tracks {
                render_track_row(app, ui, track);
            }

            ui.add_space(theme.space_md);

            // Sidecar Categories
            let sidecar_categories = [
                AssetCategory::AudioInstrument,
                AssetCategory::AudioInsert,
                AssetCategory::VisualInstrument,
                AssetCategory::VisualInsert,
            ];

            for cat in sidecar_categories {
                let mut cat_items: Vec<_> = store_list
                    .iter()
                    .filter(|d| d.sidecar_type.category() == cat)
                    .cloned()
                    .collect();

                if !search_q.is_empty() {
                    cat_items.retain(|d| {
                        d.id.to_lowercase().contains(search_q)
                            || d.name.to_lowercase().contains(search_q)
                            || d.description.to_lowercase().contains(search_q)
                            || d.tags.iter().any(|t| t.to_lowercase().contains(search_q))
                    });
                }

                if cat_items.is_empty() {
                    continue;
                }

                ui.label(
                    RichText::new(format!("━━━ {} ({}) ━━━", cat.name().to_uppercase(), cat_items.len()))
                        .size(theme.type_caption)
                        .strong()
                        .color(theme.accent),
                );
                ui.add_space(theme.space_xs);

                for descriptor in cat_items {
                    render_sidecar_card_in_library(app, ui, &descriptor);
                    ui.add_space(theme.space_sm);
                }

                ui.add_space(theme.space_sm);
            }
        });
}

fn render_sidecar_card_in_library(
    app: &mut InspectorApp,
    ui: &mut Ui,
    descriptor: &sidecar_sdk::SidecarDescriptor,
) {
    let theme = app.theme;

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .stroke(theme.border_stroke)
        .inner_margin(Margin::same(theme.space_sm))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&descriptor.name)
                        .strong()
                        .size(theme.type_caption + 1.0)
                        .color(theme.text_primary),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let cat = descriptor.sidecar_type.category();
                    let (cat_label, cat_color) = match cat {
                        AssetCategory::AudioInstrument => ("AUDIO INSTRUMENT", theme.deck_colors[0]),
                        AssetCategory::AudioInsert => ("AUDIO INSERT", theme.accent),
                        AssetCategory::VisualInstrument => ("VISUAL INSTRUMENT", theme.warning),
                        AssetCategory::VisualInsert => ("VISUAL INSERT", theme.deck_colors[1]),
                        AssetCategory::AudioFiles => ("AUDIO FILES", theme.text_secondary),
                    };

                    Frame::none()
                        .fill(cat_color.linear_multiply(0.15))
                        .rounding(Rounding::same(theme.radius_sm))
                        .inner_margin(Margin::symmetric(theme.space_xs, 2.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(cat_label)
                                    .size(theme.type_caption - 1.0)
                                    .strong()
                                    .color(cat_color),
                            );
                        });
                });
            });

            ui.add_space(2.0);

            ui.label(
                RichText::new(&descriptor.description)
                    .size(theme.type_caption)
                    .color(theme.text_secondary),
            );

            ui.add_space(theme.space_xs);

            // Action Buttons
            ui.horizontal(|ui| {
                let cat = descriptor.sidecar_type.category();
                if cat == AssetCategory::AudioInstrument {
                    let slot = app.composer.selected_composer_track.unwrap_or(0);
                    if ui.button(RichText::new(format!("→ LOAD TO TRACK {}", slot + 1)).size(theme.type_caption))
                        .on_hover_text("Assign this instrument sidecar to selected composer track")
                        .clicked()
                    {
                        app.composer.track_targets[slot] = descriptor.id.clone();
                    }
                } else if cat == AssetCategory::VisualInstrument || cat == AssetCategory::VisualInsert {
                    if ui.button(RichText::new("→ LOAD TO VISUAL MIXER").size(theme.type_caption))
                        .on_hover_text("Open in Visual Mixer surface")
                        .clicked()
                    {
                        app.active_view = crate::View::Visuals;
                    }
                } else if cat == AssetCategory::AudioInsert {
                    ui.label(RichText::new("LOAD TO DECK:").size(theme.type_caption).color(theme.text_disabled));
                    for (i, &deck_char) in ['A', 'B', 'C', 'D'].iter().enumerate() {
                        let deck_color = theme.deck_colors[i];
                        if ui.button(
                            RichText::new(format!("DECK {}", deck_char))
                                .size(theme.type_caption)
                                .color(deck_color),
                        ).on_hover_text(format!("Load {} onto Deck {}", descriptor.name, deck_char)).clicked() {
                            app.decks.deck_inserts[i].push(descriptor.name.clone());
                            app.decks.deck_insert_params[i].push([0.5; 8]);
                        }
                    }
                }
            });
        });
}

/// One library row, plus its details when expanded.
fn render_track_row(app: &mut InspectorApp, ui: &mut Ui, track: &nullherz_dna::LibraryTrack) {
    let theme = app.theme;
    let is_expanded = app.library.expanded_track == Some(track.id);
    let (rect, res) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), TRACK_ROW_H),
        egui::Sense::click(),
    );

    let hover_alpha = ui.ctx().animate_bool(res.id, res.hovered());
    if hover_alpha > 0.0 {
        ui.painter().rect_filled(rect, theme.radius_sm, Color32::from_white_alpha((hover_alpha * 15.0) as u8));
    }
    if app.library.selected_library_track == Some(track.id) {
        ui.painter().rect_filled(rect, theme.radius_sm, theme.accent.linear_multiply(0.10));
    }

    // Symmetric padding. The row used to pad only on the left, so the delete
    // button sat flush against the panel edge.
    let pad = theme.space_sm;
    let mut toggled = false;
    ui.child_ui(rect.shrink2(egui::vec2(pad, 0.0)), Layout::left_to_right(Align::Center)).horizontal(|ui| {
        // Details toggle — an explicit affordance, so opening details and
        // selecting a track stay separate actions.
        let chevron = if is_expanded { egui_phosphor::regular::CARET_DOWN } else { egui_phosphor::regular::CARET_RIGHT };
        if ui
            .add(egui::Button::new(RichText::new(chevron).size(theme.type_caption)).frame(false))
            .on_hover_text(if is_expanded { "Hide details" } else { "Show details" })
            .clicked()
        {
            toggled = true;
        }

        let is_loaded = app.decks.now_playing.iter().any(|np| np.as_ref() == Some(&track.id));
        let text_color = if is_loaded { theme.accent } else { theme.text_primary };

        // Reserve the right-hand controls explicitly rather than by magic
        // constant, so a longer BPM or a wider sparkline cannot overlap the title.
        const RIGHT_CONTROLS_W: f32 = 118.0;
        let left_budget = (rect.width() - RIGHT_CONTROLS_W - pad * 2.0).max(40.0);
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
            let bpm_text = if track.metadata.bpm >= 20.0 {
                format!("{:.0}", track.metadata.bpm)
            } else {
                "—".to_string()
            };
            ui.label(RichText::new(bpm_text).monospace().size(theme.type_caption).color(theme.text_secondary));

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

    if toggled {
        if is_expanded {
            app.library.expanded_track = None;
        } else {
            app.library.expanded_track = Some(track.id);
            // Expanding IS inspecting. `cached_inspected_track` is synced from
            // this in the frame loop, and it is what the editable fields below
            // bind to — without it the panel would open with stale content.
            app.library.selected_library_track = Some(track.id);
        }
    } else if res.clicked() {
        app.library.selected_library_track = Some(track.id);
    }

    // Double-click still loads to the focused deck — the primary gesture, kept
    // exactly as it was so the accordion does not cost anyone their muscle memory.
    if res.double_clicked() {
        let deck_idx = app.decks.focused_deck;
        if deck_idx < 4 {
            let deck_char = (b'A' + deck_idx as u8) as char;
            let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                nullherz_traits::PerformanceCommand::LoadTrackToDeck { deck_id: deck_char, sample_id: track.id },
            ));
            app.decks.now_playing[deck_idx] = Some(track.id);
        }
    }

    let detail_bottom = if is_expanded {
        render_track_details(app, ui, track)
    } else {
        rect.bottom()
    };
    ui.painter().hline(rect.x_range(), detail_bottom, Stroke::new(1.0_f32, theme.border));
}

/// The track inspector, inline under its own row.
fn render_track_details(app: &mut InspectorApp, ui: &mut Ui, track: &nullherz_dna::LibraryTrack) -> f32 {
    let theme = app.theme;
    let mut save_clicked = false;
    let mut preview_clicked = false;
    let mut energy_clicked = false;
    let mut edited: Option<nullherz_dna::LibraryTrack> = None;

    let editable = app
        .library
        .cached_inspected_track
        .as_ref()
        .map(|t| t.id == track.id)
        .unwrap_or(false);

    let frame_res = Frame::none()
        .fill(theme.bg_inset)
        .rounding(Rounding::same(theme.radius_sm))
        .inner_margin(Margin::same(theme.space_sm))
        .stroke(Stroke::new(1.0, theme.border))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());

            if editable {
                let mut t = app.library.cached_inspected_track.take().expect("checked above");
                ui.horizontal(|ui| {
                    ui.label(RichText::new("TITLE").size(theme.type_caption).color(theme.text_disabled));
                    ui.add_sized([ui.available_width() - 50.0, 18.0], egui::TextEdit::singleline(&mut t.title));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("ARTIST").size(theme.type_caption).color(theme.text_disabled));
                    ui.add_sized([ui.available_width() - 50.0, 18.0], egui::TextEdit::singleline(&mut t.artist));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("GENRE").size(theme.type_caption).color(theme.text_disabled));
                    ui.add_sized([ui.available_width() - 100.0, 18.0], egui::TextEdit::singleline(&mut t.genre));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(RichText::new("SAVE").size(theme.type_caption)).clicked() {
                            save_clicked = true;
                        }
                    });
                });
                edited = Some(t);
            } else {
                ui.label(RichText::new(&track.title).strong().size(theme.type_caption));
                ui.label(RichText::new(&track.artist).size(theme.type_caption).color(theme.text_secondary));
            }

            ui.add_space(2.0);

            let m = &track.metadata;
            let sr = m.sample_rate.max(1);
            let secs = m.total_samples as f32 / sr as f32;
            ui.horizontal_wrapped(|ui| {
                let mut kv = |k: &str, v: String| {
                    ui.label(RichText::new(k).size(theme.type_caption).color(theme.text_disabled));
                    ui.label(RichText::new(v).size(theme.type_caption).monospace().color(theme.text_secondary));
                    ui.add_space(theme.space_xs);
                };
                kv("LEN", format!("{}:{:02}", (secs as u32) / 60, (secs as u32) % 60));
                kv("RATE", format!("{sr} Hz"));
                kv("CH", format!("{}", m.channels));
                if m.bpm >= 20.0 { kv("BPM", format!("{:.1}", m.bpm)); }
                if let Some(key) = m.root_key { kv("KEY", format!("{key:.0}")); }
                if !track.album.is_empty() { kv("ALBUM", track.album.clone()); }
            });
            ui.add(egui::Label::new(RichText::new(&track.path).size(9.0).color(theme.text_disabled)).truncate(true));

            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("GENETIC PROFILE").size(theme.type_caption).strong().color(theme.accent));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(RichText::new("⚡ ENERGY MATCH").size(theme.type_caption))
                        .on_hover_text("Generate a smart crate with similar energy").clicked() { energy_clicked = true; }
                    if ui.button(RichText::new("▶ PREVIEW").size(theme.type_caption)).clicked() { preview_clicked = true; }
                });
            });
            egui::Grid::new(format!("dna_grid_{}", track.id))
                .num_columns(2)
                .spacing([theme.space_sm, 1.0])
                .show(ui, |ui| {
                    let d = &track.metadata.dna;
                    for (label, val, text, color) in [
                        ("Loudness", ((d.perception.lufs_integrated + 24.0) / 24.0).clamp(0.0, 1.0), format!("{:.1} LUFS", d.perception.lufs_integrated), theme.accent),
                        ("Crest Factor", (d.perception.crest_factor_db / 12.0).clamp(0.0, 1.0), format!("{:.1} dB", d.perception.crest_factor_db), theme.warning),
                        ("Brightness", d.perception.brightness, format!("{:.0}%", d.perception.brightness * 100.0), theme.deck_colors[0]),
                        ("Energy", d.perception.perceptual_energy, format!("{:.0}%", d.perception.perceptual_energy * 100.0), theme.success),
                        ("Flatness", d.perception.spectral_flatness, format!("{:.2}", d.perception.spectral_flatness), theme.deck_colors[2]),
                        ("Syncopation", d.rhythmic.syncopation_index, format!("{:.0}%", d.rhythmic.syncopation_index * 100.0), theme.deck_colors[1]),
                    ] {
                        ui.label(RichText::new(label).size(theme.type_caption));
                        ui.horizontal(|ui| {
                            ui.add(egui::ProgressBar::new(val.clamp(0.0, 1.0)).desired_height(6.0).fill(color));
                            ui.label(RichText::new(text).monospace().size(theme.type_caption).color(theme.text_secondary));
                        });
                        ui.end_row();
                    }
                });

            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("→ SAMPLER").size(theme.type_caption)).clicked() {
                    app.sampler.source_track = Some(track.id);
                    app.active_view = crate::View::Sampler;
                }
                if ui.button(RichText::new("→ EDITOR").size(theme.type_caption)).clicked() {
                    app.library.selected_library_track = Some(track.id);
                    app.active_view = crate::View::Editor;
                }
                if ui.button(RichText::new("→ COMPOSER").size(theme.type_caption))
                    .on_hover_text("Load into the selected sequencer track").clicked()
                {
                    let slot = app.composer.selected_composer_track.unwrap_or(0);
                    if slot < app.composer.track_sources.len() {
                        app.composer.track_sources[slot] = Some(track.id);
                        let grid_deck = app.decks.focused_deck.min(3);
                        if app.composer.sequencer_grid[grid_deck][slot].iter().all(|&v| v == 0.0) {
                            for b in 0..16 {
                                app.composer.sequencer_grid[grid_deck][slot][b] = 1.0;
                            }
                        }
                    }
                    app.active_view = crate::View::Composer;
                }

                let is_demixed = track.stems.is_some();
                let stem_button_text = if is_demixed { "✓ STEMS READY" } else { "⚡ SEPARATE STEMS" };
                if ui.button(RichText::new(stem_button_text).size(theme.type_caption).color(if is_demixed { theme.success } else { theme.accent }))
                    .on_hover_text("Extract 12-stem demixed components for multi-tier stem playback")
                    .clicked()
                {
                    let _ = app.command_sender.send(nullherz_traits::Command::Resource(
                        nullherz_traits::ResourceCommand::ExtractStems { track_id: track.id }
                    ));
                }
            });

            if let Some(ref stem_set) = track.stems {
                ui.add_space(theme.space_xs);
                ui.label(RichText::new(format!("EXTRACTED STEMS ({})", stem_set.stems.len())).size(theme.type_caption).strong().color(theme.accent));

                for (s_idx, single_stem) in stem_set.stems.iter().enumerate() {
                    let stem_id = track.id.wrapping_add((s_idx as u64 + 1) * 10000);
                    let stem_color = crate::views::dj_studio::render::stem_color_for_classif(single_stem.classification);
                    let label = crate::views::dj_studio::render::stem_label_for_classif(single_stem.classification);

                    Frame::none()
                        .fill(theme.bg_surface)
                        .stroke(Stroke::new(1.0, theme.border))
                        .rounding(Rounding::same(theme.radius_sm))
                        .inner_margin(Margin::symmetric(theme.space_xs, 2.0))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    Frame::none()
                                        .fill(stem_color.linear_multiply(0.2))
                                        .rounding(Rounding::same(theme.radius_sm))
                                        .inner_margin(Margin::symmetric(4.0, 1.0))
                                        .show(ui, |ui| {
                                            ui.label(RichText::new(label).strong().size(theme.type_caption).color(stem_color));
                                        });

                                    ui.add(egui::Label::new(
                                        RichText::new(format!("{:.1} LUFS | {:.1} dB", single_stem.lufs_integrated, single_stem.peak_db))
                                            .monospace().size(9.0).color(theme.text_secondary)
                                    ).truncate(true));
                                });

                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new("LOAD DECK:").size(theme.type_caption).color(theme.text_disabled));
                                    for (i, &deck_char) in ['A', 'B', 'C', 'D'].iter().enumerate() {
                                        let deck_color = theme.deck_colors[i];
                                        if ui.button(
                                            RichText::new(format!("DECK {}", deck_char))
                                                .size(theme.type_caption)
                                                .color(deck_color),
                                        ).on_hover_text(format!("Load {} stem onto Deck {}", label, deck_char)).clicked() {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                                                nullherz_traits::PerformanceCommand::LoadTrackToDeck {
                                                    deck_id: deck_char,
                                                    sample_id: stem_id,
                                                }
                                            ));
                                            app.decks.now_playing[i] = Some(stem_id);
                                            app.decks.cached_tracks[i] = app.get_cached_track(stem_id).or_else(|| {
                                                let mut stem_as_track = track.clone();
                                                stem_as_track.id = stem_id;
                                                stem_as_track.title = format!("{} [{}]", track.title, label);
                                                Some(stem_as_track)
                                            });
                                        }
                                    }

                                    if ui.button(RichText::new("→ SAMPLER").size(theme.type_caption)).clicked() {
                                        app.sampler.source_track = Some(stem_id);
                                        app.active_view = crate::View::Sampler;
                                    }

                                    if ui.button(RichText::new("→ COMPOSER").size(theme.type_caption)).clicked() {
                                        let slot = app.composer.selected_composer_track.unwrap_or(0);
                                        if slot < app.composer.track_sources.len() {
                                            app.composer.track_sources[slot] = Some(stem_id);
                                        }
                                        app.active_view = crate::View::Composer;
                                    }

                                    if ui.button(RichText::new("▶ PREVIEW").size(theme.type_caption)).clicked() {
                                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                                            nullherz_traits::PerformanceCommand::Preview { sample_id: stem_id }
                                        ));
                                    }
                                });
                            });
                        });
                    ui.add_space(2.0);
                }
            }
        });

    if let Some(t) = edited {
        if save_clicked {
            let _ = app.library_db.save_track(&t);
            app.library.library_needs_refresh = true;
        }
        app.library.cached_inspected_track = Some(t);
    }
    if preview_clicked {
        let _ = app.command_sender.send(nullherz_traits::Command::Performance(
            nullherz_traits::PerformanceCommand::Preview { sample_id: track.id }));
    }
    if energy_clicked {
        let tracks = app.library.cached_library_raw.clone();
        let new_crate = nullherz_dna::SmartCrateManager::generate_energy_matched_crate(track, tracks, 0.7);
        let _ = app.library_db.save_smart_crate(&new_crate);
        app.trigger_library_refresh();
    }

    frame_res.response.rect.bottom()
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
