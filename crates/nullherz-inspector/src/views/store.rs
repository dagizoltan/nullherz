use egui::{RichText, Ui, ScrollArea, Layout, Align, Frame, Margin, Rounding};
use crate::InspectorApp;
use sidecar_sdk::SidecarType;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    ui.vertical(|ui| {
        // 1. Search + Tag Filter Chips Section
        render_filter_bar(app, ui);

        ui.add_space(theme.space_sm);
        ui.separator();
        ui.add_space(theme.space_sm);

        // 2. Sidecar Catalog List
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

    // Tag Filter Chips
    ui.label(
        RichText::new("TAG FILTERS")
            .size(theme.type_caption)
            .strong()
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(theme.space_xs, theme.space_xs);

        let tags = [
            ("ALL", None),
            ("INSERT", Some("insert")),
            ("INSTRUMENT", Some("instrument")),
            ("NEURAL", Some("neural")),
            ("TCN", Some("tcn")),
            ("DELAY", Some("delay")),
            ("EQ", Some("eq")),
            ("REAL-TIME", Some("real-time")),
        ];

        for (label, tag_opt) in tags {
            let is_selected = match (&app.store.active_tag_filter, tag_opt) {
                (None, None) => true,
                (Some(a), Some(b)) => a.as_str().eq_ignore_ascii_case(b),
                _ => false,
            };

            if ui.selectable_label(is_selected, label).clicked() {
                app.store.active_tag_filter = tag_opt.map(|s| s.to_string());
            }
        }
    });
}

fn render_catalog_list(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    let mut descriptors = if let Some(ref tag) = app.store.active_tag_filter {
        app.store.store_catalog.filter_by_tag(tag)
    } else {
        app.store.store_catalog.list()
    };

    if !app.store.search_query.trim().is_empty() {
        let q = app.store.search_query.to_lowercase();
        descriptors.retain(|d| {
            d.id.to_lowercase().contains(&q)
                || d.name.to_lowercase().contains(&q)
                || d.description.to_lowercase().contains(&q)
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
            for descriptor in descriptors {
                render_sidecar_card(app, ui, &descriptor);
                ui.add_space(theme.space_sm);
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
                        SidecarType::Instrument => ("INSTRUMENT", theme.deck_colors[0]),
                        SidecarType::Insert => ("INSERT", theme.accent),
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
                if descriptor.sidecar_type == SidecarType::Instrument {
                    let slot = app.composer.selected_composer_track.unwrap_or(0);
                    if ui.button(RichText::new(format!("→ LOAD TO TRACK {}", slot + 1)).size(theme.type_caption))
                        .on_hover_text("Assign this instrument sidecar to selected composer track")
                        .clicked()
                    {
                        app.composer.track_targets[slot] = descriptor.id.clone();
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
                            let mut name_bytes = [0u8; 32];
                            let b = descriptor.id.as_bytes();
                            let len = b.len().min(32);
                            name_bytes[..len].copy_from_slice(&b[..len]);

                            let deck_str = format!("deck_{}_insert", deck_char.to_ascii_lowercase());
                            let node_idx = app.get_node_id(&deck_str).unwrap_or(i as u32 * 4 + 2);

                            let _ = app.command_sender.send(nullherz_traits::Command::Core(
                                nullherz_traits::CoreCommand::HotLoadSidecar {
                                    name: name_bytes,
                                    node_idx,
                                }
                            ));
                        }
                    }
                }
            });
        });
}
