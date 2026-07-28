use egui::{Color32, RichText, Ui, ScrollArea, Layout, Align, Stroke, Frame, Margin, Rounding};
use crate::InspectorApp;
use nullherz_dna::GeneticLibrary;

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

        render_track_list(app, ui);
    });
}

fn render_crates_and_smart_crates_section(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    // 1. Crates Header
    ui.label(
        RichText::new(format!("{} CRATES", egui_phosphor::regular::FOLDER))
            .size(theme.type_caption)
            .strong()
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    // Crates Wrapping Grid
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(theme.space_xs, theme.space_xs);

        let is_all = app.library.active_crate.is_none();
        if ui.selectable_label(is_all, format!("{} ALL", egui_phosphor::regular::PACKAGE)).clicked() {
            app.library.active_crate = None;
            app.library.library_needs_refresh = true;
        }

        let crates = &app.library.cached_crates;
        for crate_name in crates {
            let is_selected = app.library.active_crate.as_deref() == Some(crate_name.as_str());
            if ui.selectable_label(is_selected, format!("{} {}", egui_phosphor::regular::TAG, crate_name)).clicked() {
                app.library.active_crate = Some(crate_name.clone());
                app.library.library_needs_refresh = true;
            }
        }
    });

    ui.add_space(theme.space_sm);

    // 2. Smart Crates Header + NEW button
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{} SMART CRATES", egui_phosphor::regular::STAR))
                .size(theme.type_caption)
                .strong()
                .color(theme.text_secondary),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(RichText::new("+ NEW").size(theme.type_caption)).clicked() {
                app.library.smart_crate_builder_open = !app.library.smart_crate_builder_open;
            }
        });
    });
    ui.add_space(theme.space_xs);

    // Smart Crates Wrapping Grid
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(theme.space_xs, theme.space_xs);

        let smart_crates = &app.library.cached_smart_crates;
        for smart in smart_crates {
            let is_selected = app.library.active_crate.as_deref() == Some(smart.name.as_str());
            if ui.selectable_label(is_selected, format!("{} {}", egui_phosphor::regular::STAR, smart.name)).clicked() {
                app.library.active_crate = Some(smart.name.clone());
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
const TRACK_DETAIL_H: f32 = 232.0;

fn render_track_list(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    if app.library.library_needs_refresh
        && app.library.bg_library_loader.is_none() {
            app.trigger_library_refresh();
        }

    // Apply client-side search + sort on top of cached_library. Search now spans
    // title / artist / album / genre (was title/artist only); sort by the
    // selected TrackSort.
    let mut displayed_tracks = app.library.cached_library.clone();
    if !app.library.search_query.trim().is_empty() {
        let q = app.library.search_query.to_lowercase();
        displayed_tracks.retain(|t| {
            t.title.to_lowercase().contains(&q)
                || t.artist.to_lowercase().contains(&q)
                || t.album.to_lowercase().contains(&q)
                || t.genre.to_lowercase().contains(&q)
        });
    }
    app.library.sort.order_tracks(&mut displayed_tracks);

    ui.label(
        RichText::new(format!("{} TRACKS", displayed_tracks.len()))
            .size(theme.type_caption)
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xs);

    // Virtualised with VARIABLE row heights.
    //
    // `show_rows` needs every row the same height, which an accordion is not.
    // `show_viewport` hands us the visible rectangle instead, so the rows
    // outside it are replaced by two spacers and never laid out — a 5000-track
    // library still costs one expanded row plus a screenful.
    ScrollArea::vertical()
        .id_source("lib_scroll")
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let expanded = app.library.expanded_track;
            let row_h = |t: &nullherz_dna::LibraryTrack| -> f32 {
                if expanded == Some(t.id) { TRACK_ROW_H + TRACK_DETAIL_H } else { TRACK_ROW_H }
            };

            // Which rows intersect the viewport.
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

    if is_expanded {
        render_track_details(app, ui, track);
    }
    ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, theme.border));
}

/// The track inspector, inline under its own row.
///
/// This used to be a fixed card pinned above the list, which meant the details
/// for a track were nowhere near the track — you selected a row at the bottom
/// of a long list and read about it at the top, with the list shifting under
/// you as the card appeared and disappeared. Rendering it in the row's own
/// expansion keeps the subject and its detail in one place.
fn render_track_details(app: &mut InspectorApp, ui: &mut Ui, track: &nullherz_dna::LibraryTrack) {
    let theme = app.theme;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), TRACK_DETAIL_H),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, theme.radius_sm, theme.bg_inset);

    // Edits bind to the cached copy, which the frame loop keeps in step with
    // `selected_library_track`. If it is for some other row (or not loaded
    // yet), show the read-only facts rather than someone else's fields.
    let editable = app
        .library
        .cached_inspected_track
        .as_ref()
        .map(|t| t.id == track.id)
        .unwrap_or(false);

    let pad = theme.space_sm;
    let inner = rect.shrink2(egui::vec2(pad * 2.0, pad));
    let mut save_clicked = false;
    let mut preview_clicked = false;
    let mut energy_clicked = false;
    let mut edited: Option<nullherz_dna::LibraryTrack> = None;

    ui.child_ui(inner, Layout::top_down(Align::Min)).vertical(|ui| {
        ui.set_width(inner.width());

        if editable {
            let mut t = app.library.cached_inspected_track.take().expect("checked above");
            ui.horizontal(|ui| {
                ui.label(RichText::new("TITLE").size(theme.type_caption).color(theme.text_disabled));
                ui.text_edit_singleline(&mut t.title);
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("ARTIST").size(theme.type_caption).color(theme.text_disabled));
                ui.text_edit_singleline(&mut t.artist);
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("GENRE").size(theme.type_caption).color(theme.text_disabled));
                ui.text_edit_singleline(&mut t.genre);
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

        ui.add_space(theme.space_xs);

        let m = &track.metadata;
        let sr = m.sample_rate.max(1);
        let secs = m.total_samples as f32 / sr as f32;
        ui.horizontal_wrapped(|ui| {
            let mut kv = |k: &str, v: String| {
                ui.label(RichText::new(k).size(theme.type_caption).color(theme.text_disabled));
                ui.label(RichText::new(v).size(theme.type_caption).monospace().color(theme.text_secondary));
                ui.add_space(theme.space_sm);
            };
            kv("LEN", format!("{}:{:02}", (secs as u32) / 60, (secs as u32) % 60));
            kv("RATE", format!("{sr} Hz"));
            kv("CH", format!("{}", m.channels));
            if m.bpm >= 20.0 { kv("BPM", format!("{:.1}", m.bpm)); }
            if let Some(key) = m.root_key { kv("KEY", format!("{key:.0}")); }
            if !track.album.is_empty() { kv("ALBUM", track.album.clone()); }
        });
        ui.label(RichText::new(&track.path).size(theme.type_caption).color(theme.text_disabled));

        ui.add_space(theme.space_xs);
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
            .spacing([theme.space_md, 2.0])
            .show(ui, |ui| {
                let d = &track.metadata.dna;
                for (label, val, color) in [
                    ("Spectral tilt", (d.spectral.tilt + 1.0) / 2.0, theme.deck_colors[1]),
                    ("Syncopation", d.rhythmic.syncopation_index, theme.success),
                    ("Glitch density", d.artifacts.glitch_density, theme.deck_colors[2]),
                ] {
                    ui.label(RichText::new(label).size(theme.type_caption));
                    ui.add(egui::ProgressBar::new(val.clamp(0.0, 1.0)).desired_height(8.0).fill(color));
                    ui.end_row();
                }
            });

        ui.add_space(theme.space_xs);
        // Send the track to a tool without touching a deck — what makes the
        // sampler and composer usable standalone.
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
                }
                app.active_view = crate::View::Composer;
            }
        });
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
