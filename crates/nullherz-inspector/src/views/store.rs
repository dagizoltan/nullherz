use egui::{RichText, Ui, ScrollArea, Layout, Align, Frame, Margin, Rounding, Stroke};
use crate::InspectorApp;
use crate::state::{MainCategory, AudioSubcategory, SidecarSubcategory};
use sidecar_sdk::{AssetCategory, SidecarType, SidecarDescriptor};

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

    ui.vertical(|ui| {
        // 1. Categories Header & Selector
        render_categories_header(app, ui);

        ui.add_space(theme.space_sm);
        ui.separator();
        ui.add_space(theme.space_sm);

        // 2. Search & Filter Bar
        render_search_bar(app, ui);

        ui.add_space(theme.space_sm);

        // 3. Grid or List Catalog View
        if is_sidebar {
            render_catalog_list(app, ui);
        } else {
            render_catalog_grid(app, ui);
        }
    });
}

fn render_categories_header(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    // Main Category Selector Grid
    ui.label(
        RichText::new("MAIN CATEGORY")
            .size(theme.type_caption)
            .strong()
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    egui::Grid::new("store_main_cat_grid")
        .num_columns(2)
        .spacing([theme.space_xs, theme.space_xs])
        .show(ui, |ui| {
            for main_cat in MainCategory::all() {
                let is_selected = app.store.active_main_category == *main_cat;
                let (icon, label_text) = match main_cat {
                    MainCategory::Audio => (egui_phosphor::regular::MUSIC_NOTES, "AUDIO"),
                    MainCategory::Sidecars => (egui_phosphor::regular::PACKAGE, "SIDECARS"),
                };

                let bg = if is_selected { theme.accent.linear_multiply(0.18) } else { theme.bg_inset };
                let border = if is_selected { theme.accent } else { theme.border_stroke.color };

                let card = Frame::none()
                    .fill(bg)
                    .rounding(Rounding::same(theme.radius_sm))
                    .stroke(Stroke::new(1.0_f32, border))
                    .inner_margin(Margin::symmetric(theme.space_sm, theme.space_xs));

                let resp = card.show(ui, |ui| {
                    ui.set_width((ui.available_width() - theme.space_xs) * 0.5);
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new(format!("{} {}", icon, label_text))
                                .size(theme.type_caption)
                                .strong()
                                .color(if is_selected { theme.accent } else { theme.text_primary }),
                        );
                    });
                }).response;

                if resp.interact(egui::Sense::click()).clicked() {
                    app.store.active_main_category = *main_cat;
                }
            }
        });

    ui.add_space(theme.space_sm);

    // Subcategories Grid
    ui.label(
        RichText::new("SUBCATEGORIES")
            .size(theme.type_caption)
            .strong()
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    let sub_cols = 2;
    egui::Grid::new("store_sub_cat_grid")
        .num_columns(sub_cols)
        .spacing([theme.space_xs, theme.space_xs])
        .show(ui, |ui| {
            match app.store.active_main_category {
                MainCategory::Audio => {
                    for (idx, sub) in AudioSubcategory::all().iter().enumerate() {
                        let is_selected = app.store.active_audio_sub == *sub;
                        let icon = match sub {
                            AudioSubcategory::All => egui_phosphor::regular::STACK,
                            AudioSubcategory::Tracks => egui_phosphor::regular::DISC,
                            AudioSubcategory::Samples => egui_phosphor::regular::WAVEFORM,
                            AudioSubcategory::Stems => egui_phosphor::regular::LIGHTNING,
                        };

                        let label = format!("{} {}", icon, sub.name().to_uppercase());
                        let bg = if is_selected { theme.accent.linear_multiply(0.18) } else { theme.bg_inset };
                        let border = if is_selected { theme.accent } else { theme.border_stroke.color };

                        let card = Frame::none()
                            .fill(bg)
                            .rounding(Rounding::same(theme.radius_sm))
                            .stroke(Stroke::new(1.0_f32, border))
                            .inner_margin(Margin::symmetric(theme.space_xs, 4.0));

                        let resp = card.show(ui, |ui| {
                            ui.set_width((ui.available_width() - theme.space_xs) * 0.5);
                            ui.add(egui::Label::new(
                                RichText::new(&label)
                                    .size(theme.type_caption - 1.0)
                                    .strong()
                                    .color(if is_selected { theme.accent } else { theme.text_primary }),
                            ).truncate());
                        }).response;

                        if resp.interact(egui::Sense::click()).clicked() {
                            app.store.active_audio_sub = *sub;
                        }

                        if (idx + 1) % sub_cols == 0 {
                            ui.end_row();
                        }
                    }
                }
                MainCategory::Sidecars => {
                    for (idx, sub) in SidecarSubcategory::all().iter().enumerate() {
                        let is_selected = app.store.active_sidecar_sub == *sub;
                        let icon = match sub {
                            SidecarSubcategory::All => egui_phosphor::regular::PACKAGE,
                            SidecarSubcategory::AudioInstruments => egui_phosphor::regular::PIANO_KEYS,
                            SidecarSubcategory::AudioInserts => egui_phosphor::regular::SLIDERS_HORIZONTAL,
                            SidecarSubcategory::VisualInstruments => egui_phosphor::regular::APERTURE,
                            SidecarSubcategory::VisualInserts => egui_phosphor::regular::EYE,
                        };

                        let label = format!("{} {}", icon, sub.name().to_uppercase());
                        let bg = if is_selected { theme.accent.linear_multiply(0.18) } else { theme.bg_inset };
                        let border = if is_selected { theme.accent } else { theme.border_stroke.color };

                        let card = Frame::none()
                            .fill(bg)
                            .rounding(Rounding::same(theme.radius_sm))
                            .stroke(Stroke::new(1.0_f32, border))
                            .inner_margin(Margin::symmetric(theme.space_xs, 4.0));

                        let resp = card.show(ui, |ui| {
                            ui.set_width((ui.available_width() - theme.space_xs) * 0.5);
                            ui.add(egui::Label::new(
                                RichText::new(&label)
                                    .size(theme.type_caption - 1.0)
                                    .strong()
                                    .color(if is_selected { theme.accent } else { theme.text_primary }),
                            ).truncate());
                        }).response;

                        if resp.interact(egui::Sense::click()).clicked() {
                            app.store.active_sidecar_sub = *sub;
                        }

                        if (idx + 1) % sub_cols == 0 {
                            ui.end_row();
                        }
                    }
                }
            }
        });
}

fn render_search_bar(app: &mut InspectorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
        ui.add_space(4.0);
        ui.text_edit_singleline(&mut app.store.search_query);
        if !app.store.search_query.is_empty() {
            if ui.button(egui_phosphor::regular::X).clicked() {
                app.store.search_query.clear();
            }
        }
    });
}

fn get_filtered_descriptors(app: &InspectorApp) -> Vec<SidecarDescriptor> {
    let mut descriptors = app.store.store_catalog.list();

    match app.store.active_main_category {
        MainCategory::Audio => {
            // Under Audio main category, if looking for audio items in catalog:
            // descriptors with category AudioFiles or AudioInstrument/Insert if specified
            match app.store.active_audio_sub {
                AudioSubcategory::All => {
                    descriptors.retain(|d| d.sidecar_type.category() == AssetCategory::AudioFiles || d.has_tag("audio") || d.has_tag("sample") || d.has_tag("stem"));
                }
                AudioSubcategory::Tracks => {
                    descriptors.retain(|d| d.has_tag("track") || d.has_tag("demo"));
                }
                AudioSubcategory::Samples => {
                    descriptors.retain(|d| d.has_tag("sample") || d.has_tag("drum"));
                }
                AudioSubcategory::Stems => {
                    descriptors.retain(|d| d.has_tag("stem") || d.has_tag("demixed"));
                }
            }
        }
        MainCategory::Sidecars => {
            match app.store.active_sidecar_sub {
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
        }
    }

    if !app.store.search_query.trim().is_empty() {
        let q = app.store.search_query.to_lowercase();
        descriptors.retain(|d| {
            d.id.to_lowercase().contains(&q)
                || d.name.to_lowercase().contains(&q)
                || d.description.to_lowercase().contains(&q)
                || d.tags.iter().any(|t| t.to_lowercase().contains(&q))
        });
    }

    descriptors
}

// ============================================================================
// GRID VIEW RENDERING (Main Store Page)
// ============================================================================

fn render_catalog_grid(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    let descriptors = get_filtered_descriptors(app);

    ui.label(
        RichText::new(format!("{} CATALOG MODULES AVAILABLE", descriptors.len()))
            .size(theme.type_caption)
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ScrollArea::vertical()
        .id_source("store_grid_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let avail_w = ui.available_width();
            let card_w = 360.0;
            let cols = ((avail_w + theme.space_sm) / (card_w + theme.space_sm)).floor().max(1.0) as usize;

            egui::Grid::new("store_catalog_card_grid")
                .num_columns(cols)
                .spacing([theme.space_sm, theme.space_sm])
                .show(ui, |ui| {
                    for (idx, descriptor) in descriptors.iter().enumerate() {
                        render_store_grid_card(app, ui, descriptor, card_w);
                        if (idx + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
}

fn render_store_grid_card(
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
        .inner_margin(Margin::same(theme.space_md))
        .show(ui, |ui| {
            ui.set_width(card_w);
            ui.set_min_height(210.0);

            ui.vertical(|ui| {
                // Header: Name + Category badge
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&descriptor.name)
                            .strong()
                            .size(theme.type_caption + 1.0)
                            .color(theme.text_primary),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (type_label, type_color) = match descriptor.sidecar_type {
                            SidecarType::AudioInstrument | SidecarType::Instrument => ("AUDIO INSTRUMENT", theme.deck_colors[0]),
                            SidecarType::AudioInsert | SidecarType::Insert => ("AUDIO INSERT", theme.accent),
                            SidecarType::VisualGenerator => ("VISUAL GENERATOR", theme.warning),
                            SidecarType::VisualInsert => ("VISUAL INSERT", theme.deck_colors[1]),
                            SidecarType::NeuralProcessor => ("NEURAL DSP", theme.deck_colors[2]),
                            SidecarType::NeuralAnalyzer => ("ANALYZER", theme.success),
                        };

                        Frame::none()
                            .fill(type_color.linear_multiply(0.15))
                            .rounding(Rounding::same(theme.radius_sm))
                            .inner_margin(Margin::symmetric(theme.space_xs, 2.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(type_label)
                                        .size(theme.type_caption - 1.0)
                                        .strong()
                                        .color(type_color),
                                );
                            });
                    });
                });

                ui.add_space(2.0);

                // ID string & Latency
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("ID: {}", descriptor.id))
                            .monospace()
                            .size(theme.type_caption - 1.0)
                            .color(theme.text_disabled),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("Latency: {} samples (<1ms)", descriptor.latency_samples))
                                .monospace()
                                .size(theme.type_caption - 1.0)
                                .color(theme.success),
                        );
                    });
                });

                ui.add_space(theme.space_xs);

                // Description
                ui.label(
                    RichText::new(&descriptor.description)
                        .size(theme.type_caption)
                        .color(theme.text_secondary),
                );

                ui.add_space(theme.space_xs);

                // Tags
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(theme.space_xs, 2.0);
                    for tag in &descriptor.tags {
                        Frame::none()
                            .fill(theme.bg_inset)
                            .rounding(Rounding::same(theme.radius_sm))
                            .inner_margin(Margin::symmetric(4.0, 1.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!("#{}", tag))
                                        .size(theme.type_caption - 1.0)
                                        .color(theme.text_secondary),
                                );
                            });
                    }
                });

                ui.add_space(theme.space_sm);

                // 1-Click Claim / Installed status
                ui.horizontal(|ui| {
                    let installed_manifest_path = std::path::Path::new("storage/sidecars").join(format!("{}.json", descriptor.id));
                    let installed_bin_path = std::path::Path::new("storage/sidecars").join(&descriptor.id);
                    let is_installed = installed_manifest_path.exists() || installed_bin_path.exists();

                    if is_installed {
                        Frame::none()
                            .fill(theme.success.linear_multiply(0.15))
                            .rounding(Rounding::same(theme.radius_sm))
                            .inner_margin(Margin::symmetric(theme.space_sm, theme.space_xs))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!("{} INSTALLED", egui_phosphor::regular::CHECK_CIRCLE))
                                        .size(theme.type_caption)
                                        .strong()
                                        .color(theme.success),
                                );
                            });
                    } else {
                        if ui.button(
                            RichText::new(format!("{} CLAIM FREE ITEM", egui_phosphor::regular::DOWNLOAD_SIMPLE))
                                .size(theme.type_caption)
                                .strong()
                                .color(theme.accent),
                        )
                        .on_hover_text("1-Click Download & Install into storage/sidecars/")
                        .clicked()
                        {
                            let bundle_path = std::path::Path::new("assets/store_catalog").join(format!("{}.sidecar", descriptor.id));
                            if bundle_path.exists() {
                                if let Ok(bundle) = sidecar_sdk::SidecarPackageManager::unpack_bundle(&bundle_path) {
                                    let target_dir = std::path::Path::new("storage/sidecars");
                                    if let Ok(_info) = sidecar_sdk::SidecarPackageManager::install_bundle(&bundle, target_dir) {
                                        if let Ok(db) = nullherz_dna::AssetDatabase::new("storage/db/library.db") {
                                            let asset_manifest = sidecar_sdk::asset::asset_manifest_from_pkg(&bundle.manifest);
                                            let _ = db.save_manifest(&asset_manifest);
                                        }
                                        println!("Store: Successfully claimed and installed {}", descriptor.name);
                                    }
                                }
                            } else {
                                let _ = sidecar_sdk::SidecarPackageManager::populate_store_catalog(std::path::Path::new("assets/store_catalog"));
                                if let Ok(bundle) = sidecar_sdk::SidecarPackageManager::unpack_bundle(&bundle_path) {
                                    let target_dir = std::path::Path::new("storage/sidecars");
                                    let _ = sidecar_sdk::SidecarPackageManager::install_bundle(&bundle, target_dir);
                                }
                            }
                        }
                    }
                });

                ui.add_space(theme.space_xs);

                // Quick Action Routing Buttons
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 2.0);
                    if descriptor.sidecar_type == SidecarType::AudioInstrument || descriptor.sidecar_type == SidecarType::Instrument {
                        let slot = app.composer.selected_composer_track.unwrap_or(0);
                        if ui.button(RichText::new(format!("→ LOAD TO TRACK {}", slot + 1)).size(theme.type_caption))
                            .on_hover_text("Assign this instrument sidecar to selected composer track")
                            .clicked()
                        {
                            app.composer.track_targets[slot] = descriptor.id.clone();
                        }
                    } else if descriptor.sidecar_type == SidecarType::VisualGenerator || descriptor.sidecar_type == SidecarType::VisualInsert {
                        if ui.button(RichText::new("→ LOAD TO VISUAL MIXER").size(theme.type_caption))
                            .on_hover_text("Open in Visual Mixer surface")
                            .clicked()
                        {
                            app.active_view = crate::View::Visuals;
                        }
                    } else {
                        ui.label(RichText::new("HOT-LOAD:").size(theme.type_caption).color(theme.text_disabled));
                        for (i, &deck_char) in ['A', 'B', 'C', 'D'].iter().enumerate() {
                            let deck_color = theme.deck_colors[i];
                            if ui.button(
                                RichText::new(format!("DECK {}", deck_char))
                                    .size(theme.type_caption)
                                    .color(deck_color),
                            ).on_hover_text(format!("Hot-load {} onto Deck {}", descriptor.name, deck_char)).clicked() {
                                let fx_slot_idx = app.decks.deck_inserts[i].len();
                                app.decks.deck_inserts[i].push(descriptor.name.clone());
                                app.decks.deck_insert_params[i].push([0.5; 8]);

                                let deck_str = format!("deck_{}_fx{}", deck_char.to_ascii_lowercase(), fx_slot_idx + 1);
                                let node_idx = app.get_node_id(&deck_str)
                                    .or_else(|| app.get_node_id(&format!("deck_{}_insert", deck_char.to_ascii_lowercase())))
                                    .unwrap_or(i as u32 * 4 + 2);

                                let p_type_id = match descriptor.id.as_str() {
                                    "neural-saturation" => Some(nullherz_traits::ProcessorTypeId::NEURAL_SATURATOR),
                                    "neural-filter" => Some(nullherz_traits::ProcessorTypeId::NEURAL_FILTER),
                                    "neural-tcn" => Some(nullherz_traits::ProcessorTypeId::NEURAL_TCN),
                                    "neural-ssm" => Some(nullherz_traits::ProcessorTypeId::NEURAL_SSM),
                                    "neural-nam" => Some(nullherz_traits::ProcessorTypeId::NEURAL_NAM),
                                    "hypernetwork-eq" => Some(nullherz_traits::ProcessorTypeId::HYPERNETWORK_EQ),
                                    "tube-preamp" => Some(nullherz_traits::ProcessorTypeId::TUBE_PREAMP),
                                    "multiband-compressor" => Some(nullherz_traits::ProcessorTypeId::MULTIBAND_COMPRESSOR),
                                    "transient-shaper" => Some(nullherz_traits::ProcessorTypeId::TRANSIENT_SHAPER),
                                    "tape-saturator" => Some(nullherz_traits::ProcessorTypeId::TAPE_SATURATOR),
                                    "mutator" => Some(nullherz_traits::ProcessorTypeId::MUTATOR),
                                    "algorithmic-reverb" => Some(nullherz_traits::ProcessorTypeId::REVERB),
                                    "algorithmic-modulation" => Some(nullherz_traits::ProcessorTypeId::MODULATION_FX),
                                    "algorithmic-delay" => Some(nullherz_traits::ProcessorTypeId::DELAY),
                                    "algorithmic-eq" => Some(nullherz_traits::ProcessorTypeId::BIQUAD),
                                    _ => None,
                                };

                                if let Some(type_id) = p_type_id {
                                    let _ = app.command_sender.send(nullherz_traits::Command::Topology(
                                        nullherz_traits::TopologyCommand::SwapProcessor {
                                            node_idx,
                                            processor_type_id: type_id,
                                        }
                                    ));
                                } else {
                                    let mut name_bytes = [0u8; 32];
                                    let b = descriptor.id.as_bytes();
                                    let len = b.len().min(32);
                                    name_bytes[..len].copy_from_slice(&b[..len]);

                                    let _ = app.command_sender.send(nullherz_traits::Command::Core(
                                        nullherz_traits::CoreCommand::HotLoadSidecar {
                                            name: name_bytes,
                                            node_idx,
                                        }
                                    ));
                                }
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

fn render_catalog_list(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    let descriptors = get_filtered_descriptors(app);

    ui.label(
        RichText::new(format!("{} MODULES AVAILABLE", descriptors.len()))
            .size(theme.type_caption)
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ScrollArea::vertical()
        .id_source("sidecar_store_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for descriptor in descriptors {
                render_sidecar_card(app, ui, &descriptor);
                ui.add_space(theme.space_sm);
            }
        });
}

fn render_sidecar_card(
    app: &mut InspectorApp,
    ui: &mut Ui,
    descriptor: &SidecarDescriptor,
) {
    let theme = app.theme;

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .stroke(theme.border_stroke)
        .inner_margin(Margin::same(theme.space_md))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Header: Name + Type Badge
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&descriptor.name)
                        .strong()
                        .size(theme.type_caption + 1.0)
                        .color(theme.text_primary),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (type_label, type_color) = match descriptor.sidecar_type {
                        SidecarType::AudioInstrument | SidecarType::Instrument => ("AUDIO INSTRUMENT", theme.deck_colors[0]),
                        SidecarType::AudioInsert | SidecarType::Insert => ("AUDIO INSERT", theme.accent),
                        SidecarType::VisualGenerator => ("VISUAL GENERATOR", theme.warning),
                        SidecarType::VisualInsert => ("VISUAL INSERT", theme.deck_colors[1]),
                        SidecarType::NeuralProcessor => ("NEURAL DSP", theme.deck_colors[2]),
                        SidecarType::NeuralAnalyzer => ("ANALYZER", theme.success),
                    };

                    Frame::none()
                        .fill(type_color.linear_multiply(0.15))
                        .rounding(Rounding::same(theme.radius_sm))
                        .inner_margin(Margin::symmetric(theme.space_xs, 2.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(type_label)
                                    .size(theme.type_caption - 1.0)
                                    .strong()
                                    .color(type_color),
                            );
                        });
                });
            });

            ui.add_space(2.0);

            // ID string + Latency
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("ID: {}", descriptor.id))
                        .monospace()
                        .size(theme.type_caption - 1.0)
                        .color(theme.text_disabled),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("Latency: {} samples (<1ms)", descriptor.latency_samples))
                            .monospace()
                            .size(theme.type_caption - 1.0)
                            .color(theme.success),
                    );
                });
            });

            ui.add_space(theme.space_xs);

            // Description
            ui.label(
                RichText::new(&descriptor.description)
                    .size(theme.type_caption)
                    .color(theme.text_secondary),
            );

            ui.add_space(theme.space_xs);

            // Tags List
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(theme.space_xs, 2.0);
                for tag in &descriptor.tags {
                    Frame::none()
                        .fill(theme.bg_inset)
                        .rounding(Rounding::same(theme.radius_sm))
                        .inner_margin(Margin::symmetric(4.0, 1.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("#{}", tag))
                                    .size(theme.type_caption - 1.0)
                                    .color(theme.text_secondary),
                            );
                        });
                }
            });

            ui.add_space(theme.space_sm);

            // 1-Click Claim / Install Status
            ui.horizontal(|ui| {
                let installed_manifest_path = std::path::Path::new("storage/sidecars").join(format!("{}.json", descriptor.id));
                let installed_bin_path = std::path::Path::new("storage/sidecars").join(&descriptor.id);
                let is_installed = installed_manifest_path.exists() || installed_bin_path.exists();

                if is_installed {
                    Frame::none()
                        .fill(theme.success.linear_multiply(0.15))
                        .rounding(Rounding::same(theme.radius_sm))
                        .inner_margin(Margin::symmetric(theme.space_sm, theme.space_xs))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("{} INSTALLED", egui_phosphor::regular::CHECK_CIRCLE))
                                    .size(theme.type_caption)
                                    .strong()
                                    .color(theme.success),
                            );
                        });
                } else {
                    if ui.button(
                        RichText::new(format!("{} CLAIM FREE ITEM", egui_phosphor::regular::DOWNLOAD_SIMPLE))
                            .size(theme.type_caption)
                            .strong()
                            .color(theme.accent),
                    )
                    .on_hover_text("1-Click Download & Install into storage/sidecars/")
                    .clicked()
                    {
                        let bundle_path = std::path::Path::new("assets/store_catalog").join(format!("{}.sidecar", descriptor.id));
                        if bundle_path.exists() {
                            if let Ok(bundle) = sidecar_sdk::SidecarPackageManager::unpack_bundle(&bundle_path) {
                                let target_dir = std::path::Path::new("storage/sidecars");
                                if let Ok(_info) = sidecar_sdk::SidecarPackageManager::install_bundle(&bundle, target_dir) {
                                    if let Ok(db) = nullherz_dna::AssetDatabase::new("storage/db/library.db") {
                                        let asset_manifest = sidecar_sdk::asset::asset_manifest_from_pkg(&bundle.manifest);
                                        let _ = db.save_manifest(&asset_manifest);
                                    }
                                    println!("Store: Successfully claimed and installed {}", descriptor.name);
                                }
                            }
                        } else {
                            let _ = sidecar_sdk::SidecarPackageManager::populate_store_catalog(std::path::Path::new("assets/store_catalog"));
                            if let Ok(bundle) = sidecar_sdk::SidecarPackageManager::unpack_bundle(&bundle_path) {
                                let target_dir = std::path::Path::new("storage/sidecars");
                                let _ = sidecar_sdk::SidecarPackageManager::install_bundle(&bundle, target_dir);
                            }
                        }
                    }
                }
            });

            ui.add_space(theme.space_xs);

            // Action Buttons
            ui.horizontal(|ui| {
                if descriptor.sidecar_type == SidecarType::AudioInstrument || descriptor.sidecar_type == SidecarType::Instrument {
                    let slot = app.composer.selected_composer_track.unwrap_or(0);
                    if ui.button(RichText::new(format!("→ LOAD TO TRACK {}", slot + 1)).size(theme.type_caption))
                        .on_hover_text("Assign this instrument sidecar to selected composer track")
                        .clicked()
                    {
                        app.composer.track_targets[slot] = descriptor.id.clone();
                    }
                } else if descriptor.sidecar_type == SidecarType::VisualGenerator || descriptor.sidecar_type == SidecarType::VisualInsert {
                    if ui.button(RichText::new("→ LOAD TO VISUAL MIXER").size(theme.type_caption))
                        .on_hover_text("Open in Visual Mixer surface")
                        .clicked()
                    {
                        app.active_view = crate::View::Visuals;
                    }
                } else {
                    ui.label(RichText::new("HOT-LOAD:").size(theme.type_caption).color(theme.text_disabled));
                    for (i, &deck_char) in ['A', 'B', 'C', 'D'].iter().enumerate() {
                        let deck_color = theme.deck_colors[i];
                        if ui.button(
                            RichText::new(format!("DECK {}", deck_char))
                                .size(theme.type_caption)
                                .color(deck_color),
                        ).on_hover_text(format!("Hot-load {} onto Deck {}", descriptor.name, deck_char)).clicked() {
                            let fx_slot_idx = app.decks.deck_inserts[i].len();
                            app.decks.deck_inserts[i].push(descriptor.name.clone());
                            app.decks.deck_insert_params[i].push([0.5; 8]);

                            let deck_str = format!("deck_{}_fx{}", deck_char.to_ascii_lowercase(), fx_slot_idx + 1);
                            let node_idx = app.get_node_id(&deck_str)
                                .or_else(|| app.get_node_id(&format!("deck_{}_insert", deck_char.to_ascii_lowercase())))
                                .unwrap_or(i as u32 * 4 + 2);

                            let p_type_id = match descriptor.id.as_str() {
                                "neural-saturation" => Some(nullherz_traits::ProcessorTypeId::NEURAL_SATURATOR),
                                "neural-filter" => Some(nullherz_traits::ProcessorTypeId::NEURAL_FILTER),
                                "neural-tcn" => Some(nullherz_traits::ProcessorTypeId::NEURAL_TCN),
                                "neural-ssm" => Some(nullherz_traits::ProcessorTypeId::NEURAL_SSM),
                                "neural-nam" => Some(nullherz_traits::ProcessorTypeId::NEURAL_NAM),
                                "hypernetwork-eq" => Some(nullherz_traits::ProcessorTypeId::HYPERNETWORK_EQ),
                                "tube-preamp" => Some(nullherz_traits::ProcessorTypeId::TUBE_PREAMP),
                                "multiband-compressor" => Some(nullherz_traits::ProcessorTypeId::MULTIBAND_COMPRESSOR),
                                "transient-shaper" => Some(nullherz_traits::ProcessorTypeId::TRANSIENT_SHAPER),
                                "tape-saturator" => Some(nullherz_traits::ProcessorTypeId::TAPE_SATURATOR),
                                "mutator" => Some(nullherz_traits::ProcessorTypeId::MUTATOR),
                                "algorithmic-reverb" => Some(nullherz_traits::ProcessorTypeId::REVERB),
                                "algorithmic-modulation" => Some(nullherz_traits::ProcessorTypeId::MODULATION_FX),
                                "algorithmic-delay" => Some(nullherz_traits::ProcessorTypeId::DELAY),
                                "algorithmic-eq" => Some(nullherz_traits::ProcessorTypeId::BIQUAD),
                                _ => None,
                            };

                            if let Some(type_id) = p_type_id {
                                let _ = app.command_sender.send(nullherz_traits::Command::Topology(
                                    nullherz_traits::TopologyCommand::SwapProcessor {
                                        node_idx,
                                        processor_type_id: type_id,
                                    }
                                ));
                            } else {
                                let mut name_bytes = [0u8; 32];
                                let b = descriptor.id.as_bytes();
                                let len = b.len().min(32);
                                name_bytes[..len].copy_from_slice(&b[..len]);

                                let _ = app.command_sender.send(nullherz_traits::Command::Core(
                                    nullherz_traits::CoreCommand::HotLoadSidecar {
                                        name: name_bytes,
                                        node_idx,
                                    }
                                ));
                            }
                        }
                    }
                }
            });
        });
}
