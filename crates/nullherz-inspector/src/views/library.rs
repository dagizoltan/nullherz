use egui::{Color32, RichText, Ui, ScrollArea, Layout, Align, Stroke, Frame, Margin, Rounding};
use crate::InspectorApp;
use crate::state::{MainCategory, AudioSubcategory, SidecarSubcategory};
use nullherz_dna::GeneticLibrary;
use sidecar_sdk::{AssetCategory, SidecarDescriptor};

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

    if let Ok(home) = std::env::var("HOME") {
        for (label, sub) in [("User Music", "Music"), ("Downloads", "Downloads"), ("Desktop", "Desktop")] {
            let user_dir = format!("{}/{}", home, sub);
            if std::path::Path::new(&user_dir).exists() {
                locations.push(AudioLocation {
                    label: label.to_string(),
                    path: user_dir,
                    is_external: false,
                });
            }
        }
    }

    let macos_system_volumes = [
        "macintosh hd", "system", "data", "preboot", "vm", "update", "recovery",
        ".trashes", ".spotlight-v100", "com.apple",
    ];

    let mount_roots = ["/media", "/run/media", "/mnt", "/Volumes"];
    for root in &mount_roots {
        let p = std::path::Path::new(root);
        if p.exists() && p.is_dir() {
            if let Ok(entries) = std::fs::read_dir(p) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let name_lower = name.to_lowercase();

                        if *root == "/Volumes" && macos_system_volumes.iter().any(|sys| name_lower.contains(sys)) {
                            continue;
                        }

                        if root.contains("media") {
                            if let Ok(sub_entries) = std::fs::read_dir(&path) {
                                for sub in sub_entries.flatten() {
                                    let sub_path = sub.path();
                                    if sub_path.is_dir() {
                                        let sub_name = sub.file_name().to_string_lossy().to_string();
                                        locations.push(AudioLocation {
                                            label: format!("External Drive ({})", sub_name),
                                            path: sub_path.to_string_lossy().to_string(),
                                            is_external: true,
                                        });
                                    }
                                }
                            }
                        } else {
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

/// Main View entry point (Grid View default)
pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    render_with_mode(app, ui, false);
}

/// Right Sidebar entry point (List View default)
pub fn render_sidebar(app: &mut InspectorApp, ui: &mut Ui) {
    render_with_mode(app, ui, true);
}

pub fn render_with_mode(app: &mut InspectorApp, ui: &mut Ui, is_sidebar: bool) {
    let theme = app.theme;

    if app.library.library_needs_refresh && app.library.bg_library_loader.is_none() {
        app.trigger_library_refresh();
    }

    ui.vertical(|ui| {
        // 1. Categories Header & Selector
        render_categories_header(app, ui);

        ui.add_space(theme.space_sm);
        ui.separator();
        ui.add_space(theme.space_sm);

        // 2. Toolbar (Search query, Sort, Ingestion path scanner)
        render_toolbar(app, ui);

        ui.add_space(theme.space_sm);

        // 3. Asset Grid (Main View) or Asset List (Sidebar)
        if is_sidebar {
            render_asset_list(app, ui);
        } else {
            render_asset_grid(app, ui);
        }
    });
}

fn render_categories_header(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    // Main Category Selector Buttons
    ui.label(
        RichText::new("MAIN CATEGORY")
            .size(theme.type_caption)
            .strong()
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(theme.space_sm, theme.space_xs);

        for main_cat in MainCategory::all() {
            let is_selected = app.library.active_main_category == *main_cat;
            let (icon, label_text) = match main_cat {
                MainCategory::Audio => (egui_phosphor::regular::MUSIC_NOTES, "AUDIO"),
                MainCategory::Sidecars => (egui_phosphor::regular::PACKAGE, "SIDECARS"),
            };

            let text = RichText::new(format!("{} {}", icon, label_text))
                .size(theme.type_caption + 1.0)
                .strong();

            if ui.selectable_label(is_selected, text).clicked() {
                app.library.active_main_category = *main_cat;
                app.library.library_needs_refresh = true;
            }
        }
    });

    ui.add_space(theme.space_sm);

    // Subcategory Chips
    ui.label(
        RichText::new("SUBCATEGORIES")
            .size(theme.type_caption)
            .strong()
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(theme.space_xs, theme.space_xs);

        match app.library.active_main_category {
            MainCategory::Audio => {
                for sub in AudioSubcategory::all() {
                    let is_selected = app.library.active_audio_sub == *sub && app.library.active_crate.is_none();
                    let icon = match sub {
                        AudioSubcategory::All => egui_phosphor::regular::STACK,
                        AudioSubcategory::Tracks => egui_phosphor::regular::DISC,
                        AudioSubcategory::Samples => egui_phosphor::regular::WAVEFORM,
                        AudioSubcategory::Stems => egui_phosphor::regular::LIGHTNING,
                    };

                    if ui.selectable_label(is_selected, format!("{} {}", icon, sub.name().to_uppercase())).clicked() {
                        app.library.active_audio_sub = *sub;
                        app.library.active_crate = None;
                        app.library.library_needs_refresh = true;
                    }
                }

                for crate_name in app.library.cached_crates.clone() {
                    if ["track", "sample", "sequence", "instrument", "insert", "visual", "stems"].contains(&crate_name.as_str()) {
                        continue;
                    }
                    let is_selected = app.library.active_crate.as_deref() == Some(crate_name.as_str());
                    if ui.selectable_label(is_selected, format!("{} {}", egui_phosphor::regular::TAG, crate_name)).clicked() {
                        app.library.active_crate = Some(crate_name);
                        app.library.library_needs_refresh = true;
                    }
                }

                for smart in app.library.cached_smart_crates.clone() {
                    let is_selected = app.library.active_crate.as_deref() == Some(smart.name.as_str());
                    if ui.selectable_label(is_selected, format!("{} {}", egui_phosphor::regular::STAR, smart.name)).clicked() {
                        app.library.active_crate = Some(smart.name);
                        app.library.library_needs_refresh = true;
                    }
                }
            }
            MainCategory::Sidecars => {
                for sub in SidecarSubcategory::all() {
                    let is_selected = app.library.active_sidecar_sub == *sub;
                    let icon = match sub {
                        SidecarSubcategory::All => egui_phosphor::regular::PACKAGE,
                        SidecarSubcategory::AudioInstruments => egui_phosphor::regular::PIANO_KEYS,
                        SidecarSubcategory::AudioInserts => egui_phosphor::regular::SLIDERS_HORIZONTAL,
                        SidecarSubcategory::VisualInstruments => egui_phosphor::regular::APERTURE,
                        SidecarSubcategory::VisualInserts => egui_phosphor::regular::EYE,
                    };

                    if ui.selectable_label(is_selected, format!("{} {}", icon, sub.name().to_uppercase())).clicked() {
                        app.library.active_sidecar_sub = *sub;
                    }
                }
            }
        }
    });
}

fn render_toolbar(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    if app.library.smart_crate_builder_open {
        render_smart_crate_builder(app, ui);
        ui.add_space(theme.space_sm);
    }

    // Row 1: Search Query + Sort
    ui.horizontal(|ui| {
        ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
        ui.add_space(theme.space_xs);
        ui.text_edit_singleline(&mut app.library.search_query);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE).on_hover_text("Refresh Library").clicked() {
                app.library.library_needs_refresh = true;
            }
            egui::ComboBox::from_id_source("lib_sort_select")
                .selected_text(track_sort_label(app.library.sort))
                .show_ui(ui, |ui| {
                    use nullherz_dna::TrackSort::*;
                    for s in [Title, Artist, Album, Genre, BpmAsc, BpmDesc, EnergyAsc, EnergyDesc] {
                        ui.selectable_value(&mut app.library.sort, s, track_sort_label(s));
                    }
                })
                .response.on_hover_text("Sort Order");
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

        egui::ComboBox::from_id_source("lib_location_select_main")
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

// ============================================================================
// GRID VIEW RENDERING (Main Page)
// ============================================================================

fn render_asset_grid(app: &mut InspectorApp, ui: &mut Ui) {
    let search_q = app.library.search_query.trim().to_lowercase();

    match app.library.active_main_category {
        MainCategory::Audio => {
            render_audio_grid(app, ui, &search_q);
        }
        MainCategory::Sidecars => {
            render_sidecar_grid(app, ui, &search_q);
        }
    }
}

fn get_filtered_audio_tracks(app: &InspectorApp, search_q: &str) -> Vec<nullherz_dna::LibraryTrack> {
    let mut tracks = app.library.cached_library.clone();

    // Filter by Subcategory
    match app.library.active_audio_sub {
        AudioSubcategory::All => {
            tracks.retain(|t| !t.path.contains("library/stems/") && !t.path.contains("stems/stem_"));
        }
        AudioSubcategory::Tracks => {
            tracks.retain(|t| !t.path.contains("samples/") && !t.path.contains("sequences/") && !t.path.contains("stems/") && t.stems.is_none());
        }
        AudioSubcategory::Samples => {
            tracks.retain(|t| (t.path.contains("samples/") || t.path.contains("sequences/")) && !t.path.contains("stems/"));
        }
        AudioSubcategory::Stems => {
            tracks.retain(|t| t.stems.is_some() && !t.path.contains("stems/stem_"));
        }
    }

    if app.library.active_crate.as_deref() == Some("stems") {
        tracks.retain(|t| t.stems.is_some() && !t.path.contains("stems/stem_"));
    }

    if !search_q.is_empty() {
        tracks.retain(|t| {
            t.title.to_lowercase().contains(search_q)
                || t.artist.to_lowercase().contains(search_q)
                || t.album.to_lowercase().contains(search_q)
                || t.genre.to_lowercase().contains(search_q)
        });
    }

    app.library.sort.order_tracks(&mut tracks);
    tracks
}

fn render_audio_grid(app: &mut InspectorApp, ui: &mut Ui, search_q: &str) {
    let theme = app.theme;
    let tracks = get_filtered_audio_tracks(app, search_q);

    ui.label(
        RichText::new(format!("{} AUDIO ITEMS AVAILABLE", tracks.len()))
            .size(theme.type_caption)
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ScrollArea::vertical()
        .id_source("lib_audio_grid_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let avail_w = ui.available_width();
            let card_w = 340.0;
            let cols = ((avail_w + theme.space_sm) / (card_w + theme.space_sm)).floor().max(1.0) as usize;

            egui::Grid::new("library_audio_card_grid")
                .num_columns(cols)
                .spacing([theme.space_sm, theme.space_sm])
                .show(ui, |ui| {
                    for (idx, track) in tracks.iter().enumerate() {
                        render_audio_card_grid_item(app, ui, track, card_w);
                        if (idx + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
}

fn render_audio_card_grid_item(
    app: &mut InspectorApp,
    ui: &mut Ui,
    track: &nullherz_dna::LibraryTrack,
    card_w: f32,
) {
    let theme = app.theme;
    let is_loaded = app.decks.now_playing.iter().any(|np| np.as_ref() == Some(&track.id));
    let is_selected = app.library.selected_library_track == Some(track.id);

    Frame::none()
        .fill(if is_selected { theme.accent.linear_multiply(0.10) } else { theme.bg_surface })
        .rounding(Rounding::same(theme.radius_md))
        .stroke(Stroke::new(1.0, if is_loaded { theme.accent } else { theme.border }))
        .inner_margin(Margin::same(theme.space_sm))
        .show(ui, |ui| {
            ui.set_width(card_w);
            ui.set_min_height(190.0);

            ui.vertical(|ui| {
                // Header: Title & Artist
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(egui_phosphor::regular::MUSIC_NOTE)
                            .size(16.0)
                            .color(if is_loaded { theme.accent } else { theme.text_secondary }),
                    );
                    ui.add(egui::Label::new(
                        RichText::new(&track.title)
                            .strong()
                            .size(theme.type_caption + 1.0)
                            .color(if is_loaded { theme.accent } else { theme.text_primary }),
                    ).truncate());

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(egui_phosphor::regular::TRASH).on_hover_text("Delete track").clicked() {
                            let _ = app.library_db.remove_track(track.id);
                            app.library.library_needs_refresh = true;
                        }
                    });
                });

                ui.label(
                    RichText::new(&track.artist)
                        .size(theme.type_caption)
                        .color(theme.text_secondary),
                );

                ui.add_space(4.0);

                // Waveform / DNA Sparklines
                let (spark_rect, _) = ui.allocate_at_least(egui::vec2(card_w - 16.0, 16.0), egui::Sense::hover());
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

                ui.add_space(4.0);

                // Track Metadata badges
                let m = &track.metadata;
                let sr = m.sample_rate.max(1);
                let secs = m.total_samples as f32 / sr as f32;
                let bpm_text = if m.bpm >= 20.0 { format!("{:.0} BPM", m.bpm) } else { "— BPM".to_string() };
                let key_text = m.root_key.map(|k| format!("KEY {:.0}", k)).unwrap_or_else(|| "KEY —".to_string());
                let len_text = format!("{}:{:02}", (secs as u32) / 60, (secs as u32) % 60);

                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} | {} | {}", bpm_text, key_text, len_text)).monospace().size(10.0).color(theme.text_secondary));
                    if track.stems.is_some() {
                        ui.add_space(theme.space_xs);
                        Frame::none()
                            .fill(theme.success.linear_multiply(0.15))
                            .rounding(Rounding::same(theme.radius_sm))
                            .inner_margin(Margin::symmetric(4.0, 1.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new("STEMS READY").size(9.0).strong().color(theme.success));
                            });
                    }
                });

                ui.add_space(theme.space_xs);

                // Quick Action Buttons
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("▶ PREVIEW").size(theme.type_caption)).clicked() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                            nullherz_traits::PerformanceCommand::Preview { sample_id: track.id }
                        ));
                    }

                    if ui.button(RichText::new("→ SAMPLER").size(theme.type_caption)).clicked() {
                        app.sampler.source_track = Some(track.id);
                        app.active_view = crate::View::Sampler;
                    }

                    if ui.button(RichText::new("→ COMPOSER").size(theme.type_caption)).clicked() {
                        let slot = app.composer.selected_composer_track.unwrap_or(0);
                        if slot < app.composer.track_sources.len() {
                            app.composer.track_sources[slot] = Some(track.id);
                        }
                        app.active_view = crate::View::Composer;
                    }
                });

                ui.add_space(2.0);

                // Load to Deck Row
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 2.0);
                    ui.label(RichText::new("LOAD:").size(theme.type_caption).color(theme.text_disabled));
                    for (i, &deck_char) in ['A', 'B', 'C', 'D'].iter().enumerate() {
                        let deck_color = theme.deck_colors[i];
                        if ui.button(
                            RichText::new(format!("DECK {}", deck_char))
                                .size(theme.type_caption)
                                .color(deck_color),
                        ).on_hover_text(format!("Load track onto Deck {}", deck_char)).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                                nullherz_traits::PerformanceCommand::LoadTrackToDeck { deck_id: deck_char, sample_id: track.id },
                            ));
                            app.decks.now_playing[i] = Some(track.id);
                        }
                    }
                });
            });
        });
}

fn get_filtered_sidecars(app: &InspectorApp, search_q: &str) -> Vec<SidecarDescriptor> {
    let mut descriptors = app.store.store_catalog.list();

    match app.library.active_sidecar_sub {
        SidecarSubcategory::All => {}
        SidecarSubcategory::AudioInstruments => {
            descriptors.retain(|d| d.sidecar_type.category() == AssetCategory::AudioInstrument);
        }
        SidecarSubcategory::AudioInserts => {
            descriptors.retain(|d| d.sidecar_type.category() == AssetCategory::AudioInsert);
        }
        SidecarSubcategory::VisualInstruments => {
            descriptors.retain(|d| d.sidecar_type.category() == AssetCategory::VisualInstrument);
        }
        SidecarSubcategory::VisualInserts => {
            descriptors.retain(|d| d.sidecar_type.category() == AssetCategory::VisualInsert);
        }
    }

    if !search_q.is_empty() {
        descriptors.retain(|d| {
            d.id.to_lowercase().contains(search_q)
                || d.name.to_lowercase().contains(search_q)
                || d.description.to_lowercase().contains(search_q)
                || d.tags.iter().any(|t| t.to_lowercase().contains(search_q))
        });
    }

    descriptors
}

fn render_sidecar_grid(app: &mut InspectorApp, ui: &mut Ui, search_q: &str) {
    let theme = app.theme;
    let descriptors = get_filtered_sidecars(app, search_q);

    ui.label(
        RichText::new(format!("{} SIDECAR MODULES AVAILABLE", descriptors.len()))
            .size(theme.type_caption)
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ScrollArea::vertical()
        .id_source("lib_sidecar_grid_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let avail_w = ui.available_width();
            let card_w = 340.0;
            let cols = ((avail_w + theme.space_sm) / (card_w + theme.space_sm)).floor().max(1.0) as usize;

            egui::Grid::new("library_sidecar_card_grid")
                .num_columns(cols)
                .spacing([theme.space_sm, theme.space_sm])
                .show(ui, |ui| {
                    for (idx, descriptor) in descriptors.iter().enumerate() {
                        render_sidecar_card_grid_item(app, ui, descriptor, card_w);
                        if (idx + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
}

fn render_sidecar_card_grid_item(
    app: &mut InspectorApp,
    ui: &mut Ui,
    descriptor: &SidecarDescriptor,
    card_w: f32,
) {
    let theme = app.theme;

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .stroke(theme.border_stroke)
        .inner_margin(Margin::same(theme.space_sm))
        .show(ui, |ui| {
            ui.set_width(card_w);
            ui.set_min_height(170.0);

            ui.vertical(|ui| {
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
                ui.horizontal_wrapped(|ui| {
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
        });
}

// ============================================================================
// LIST VIEW RENDERING (Right Sidebar)
// ============================================================================

const TRACK_ROW_H: f32 = 26.0;
const TRACK_DETAIL_H: f32 = 250.0;

fn render_asset_list(app: &mut InspectorApp, ui: &mut Ui) {
    let search_q = app.library.search_query.trim().to_lowercase();

    match app.library.active_main_category {
        MainCategory::Audio => {
            render_audio_files_list(app, ui, &search_q);
        }
        MainCategory::Sidecars => {
            render_sidecars_list(app, ui, &search_q);
        }
    }
}

fn render_audio_files_list(app: &mut InspectorApp, ui: &mut Ui, search_q: &str) {
    let theme = app.theme;
    let displayed_tracks = get_filtered_audio_tracks(app, search_q);

    ui.label(
        RichText::new(format!("{} AUDIO FILES", displayed_tracks.len()))
            .size(theme.type_caption)
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ScrollArea::vertical()
        .id_source("lib_sidebar_audio_scroll")
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

fn render_sidecars_list(app: &mut InspectorApp, ui: &mut Ui, search_q: &str) {
    let theme = app.theme;
    let descriptors = get_filtered_sidecars(app, search_q);

    ui.label(
        RichText::new(format!("{} SIDECAR MODULES", descriptors.len()))
            .size(theme.type_caption)
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ScrollArea::vertical()
        .id_source("lib_sidebar_sidecar_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for descriptor in descriptors {
                render_sidecar_card_in_library(app, ui, &descriptor);
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

    let pad = theme.space_sm;
    let mut toggled = false;
    ui.child_ui(rect.shrink2(egui::vec2(pad, 0.0)), Layout::left_to_right(Align::Center), None).horizontal(|ui| {
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

        const RIGHT_CONTROLS_W: f32 = 118.0;
        let left_budget = (rect.width() - RIGHT_CONTROLS_W - pad * 2.0).max(40.0);
        ui.allocate_ui(egui::vec2(left_budget, rect.height()), |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(&track.title).color(text_color).strong().size(theme.type_caption)).truncate());
                ui.add(egui::Label::new(RichText::new(&track.artist).color(theme.text_secondary).size(theme.type_caption)).truncate());
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
            app.library.selected_library_track = Some(track.id);
        }
    } else if res.clicked() {
        app.library.selected_library_track = Some(track.id);
    }

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
        .stroke(Stroke::new(1.0_f32, theme.border))
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
            ui.add(egui::Label::new(RichText::new(&track.path).size(9.0).color(theme.text_disabled)).truncate());

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
                        .stroke(Stroke::new(1.0_f32, theme.border))
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
                                    ).truncate());
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
