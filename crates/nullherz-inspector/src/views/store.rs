use egui::{RichText, Ui, ScrollArea, Layout, Align, Frame, Margin, Rounding};
use crate::InspectorApp;
use sidecar_sdk::{AssetCategory, SidecarType};

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    ui.vertical(|ui| {
        // 1. Search + Main Category Chips Section
        render_filter_bar(app, ui);

        ui.add_space(theme.space_sm);
        ui.separator();
        ui.add_space(theme.space_sm);

        // 2. Sidecar Catalog List Grouped or Filtered by Category
        render_catalog_list(app, ui);
    });
}

fn render_filter_bar(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    // Search Box
    ui.horizontal(|ui| {
        ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
        ui.add_space(theme.space_xs);
        ui.text_edit_singleline(&mut app.store.search_query);
        if !app.store.search_query.is_empty() {
            if ui.button(egui_phosphor::regular::X).clicked() {
                app.store.search_query.clear();
            }
        }
    });

    ui.add_space(theme.space_xs);

    // Main Category Selector Chips
    ui.label(
        RichText::new("CATEGORIES")
            .size(theme.type_caption)
            .strong()
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(theme.space_xs, theme.space_xs);

        let is_all = app.store.active_category.is_none();
        if ui.selectable_label(is_all, format!("{} ALL", egui_phosphor::regular::PACKAGE)).clicked() {
            app.store.active_category = None;
        }

        for category in AssetCategory::all() {
            let is_selected = app.store.active_category == Some(*category);
            let icon = match category {
                AssetCategory::AudioFiles => egui_phosphor::regular::MUSIC_NOTES,
                AssetCategory::AudioInsert => egui_phosphor::regular::SLIDERS_HORIZONTAL,
                AssetCategory::VisualInsert => egui_phosphor::regular::EYE,
                AssetCategory::AudioInstrument => egui_phosphor::regular::PIANO_KEYS,
                AssetCategory::VisualInstrument => egui_phosphor::regular::APERTURE,
            };

            if ui.selectable_label(is_selected, format!("{} {}", icon, category.name().to_uppercase())).clicked() {
                app.store.active_category = Some(*category);
            }
        }
    });
}

fn render_catalog_list(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    let mut descriptors = app.store.store_catalog.list();

    if let Some(cat) = app.store.active_category {
        descriptors.retain(|d| d.sidecar_type.category() == cat);
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

    ui.label(
        RichText::new(format!("{} SIDECAR MODULES AVAILABLE", descriptors.len()))
            .size(theme.type_caption)
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ScrollArea::vertical()
        .id_source("sidecar_store_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if app.store.active_category.is_some() {
                for descriptor in descriptors {
                    render_sidecar_card(app, ui, &descriptor);
                    ui.add_space(theme.space_sm);
                }
            } else {
                // Group by Category when "ALL" is selected
                let categories = [
                    AssetCategory::AudioInstrument,
                    AssetCategory::AudioInsert,
                    AssetCategory::VisualInstrument,
                    AssetCategory::VisualInsert,
                    AssetCategory::AudioFiles,
                ];

                for category in categories {
                    let items_in_cat: Vec<_> = descriptors
                        .iter()
                        .filter(|d| d.sidecar_type.category() == category)
                        .cloned()
                        .collect();

                    if items_in_cat.is_empty() {
                        continue;
                    }

                    ui.add_space(theme.space_xs);
                    ui.label(
                        RichText::new(format!("━━━ {} ({}) ━━━", category.name().to_uppercase(), items_in_cat.len()))
                            .size(theme.type_caption)
                            .strong()
                            .color(theme.accent),
                    );
                    ui.add_space(theme.space_xs);

                    for descriptor in items_in_cat {
                        render_sidecar_card(app, ui, &descriptor);
                        ui.add_space(theme.space_sm);
                    }
                }
            }
        });
}

fn render_sidecar_card(
    app: &mut InspectorApp,
    ui: &mut Ui,
    descriptor: &sidecar_sdk::SidecarDescriptor,
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
