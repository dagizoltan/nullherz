use egui::{Ui, ScrollArea, Color32, Frame, Vec2, Sense, Stroke, RichText, Margin};
use crate::InspectorApp;
use audio_core::Telemetry;
use nullherz_dna::GeneticLibrary;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    if app.decks.focused_deck >= 4 {
        app.decks.focused_deck = 0;
    }
    let deck_idx = app.decks.focused_deck;
    let deck_char = (b'A' + deck_idx as u8) as char;
    let theme = app.theme;
    let deck_color = crate::InspectorApp::deck_color(&theme, deck_idx);

    ui.horizontal(|ui| {
        ui.heading(RichText::new("ADVANCED PERFORMANCE PLAYER").strong().color(theme.text_primary).size(theme.type_heading));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format!("FOCUS: DECK {}", deck_char)).strong().color(deck_color).size(theme.type_label));
        });
    });
    ui.add_space(theme.space_sm);

    // Deck Tab Selector
    ui.horizontal(|ui| {
        for i in 0..4 {
            let active = i == app.decks.focused_deck;
            let d_char = (b'A' + i as u8) as char;
            let d_color = crate::InspectorApp::deck_color(&theme, i);
            let btn_text = format!("DECK {}", d_char);

            let btn = if active {
                egui::Button::new(RichText::new(btn_text).strong().color(Color32::BLACK).size(theme.type_label)).fill(d_color)
            } else {
                egui::Button::new(RichText::new(btn_text).color(d_color).size(theme.type_label)).fill(theme.bg_inset)
            };

            if ui.add_sized([100.0, 24.0], btn).clicked() {
                app.decks.focused_deck = i;
            }
            ui.add_space(theme.space_xs);
        }
    });
    ui.add_space(theme.space_md);

    // Advanced Player Deck Panel (Turntable + Transport Dashboard)
    Frame::none()
        .fill(theme.bg_surface)
        .stroke(theme.border_stroke)
        .rounding(theme.radius_md)
        .inner_margin(Margin::same(theme.space_md))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                // Column 1: Vinyl Jog Wheel & Physical Trajectory (turntable)
                ui.vertical(|ui| {
                    ui.set_width(180.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("VIRTUAL JOG WHEEL").color(theme.text_secondary).size(theme.type_caption));
                        ui.add_space(theme.space_xs);

                        // Render an elegant rotating Jog Wheel
                        let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions[deck_idx]).unwrap_or(0);
                        let rotation_angle = (elapsed_samples as f32 * 0.0001) % (std::f32::consts::PI * 2.0);

                        let (rect, _response) = ui.allocate_exact_size(Vec2::splat(120.0), Sense::click_and_drag());

                        // Jog Wheel outer ring
                        ui.painter().circle_stroke(rect.center(), 58.0, Stroke::new(2.0, theme.border));
                        ui.painter().circle_filled(rect.center(), 56.0, theme.bg_inset);
                        ui.painter().circle_stroke(rect.center(), 48.0, Stroke::new(1.0, theme.border));

                        // Rotational vinyl grooves
                        for radius in [12.0, 20.0, 28.0, 36.0, 44.0] {
                            ui.painter().circle_stroke(rect.center(), radius, Stroke::new(0.5, theme.border.linear_multiply(0.5)));
                        }

                        // Center hub (accent color)
                        ui.painter().circle_filled(rect.center(), 10.0, deck_color);
                        ui.painter().circle_filled(rect.center(), 3.0, Color32::BLACK);

                        // Rotational position indicator marker (Turntable tape marker)
                        let marker_len = 54.0;
                        let marker_end = rect.center() + Vec2::new(rotation_angle.cos() * marker_len, rotation_angle.sin() * marker_len);
                        ui.painter().line_segment([rect.center(), marker_end], Stroke::new(2.0, deck_color));

                        ui.add_space(theme.space_sm);
                        ui.label(RichText::new("SCRATCH / JOG").color(theme.text_disabled).size(theme.type_caption));
                    });
                });

                ui.add_space(theme.space_md);

                // Column 2: Dashboard (Waveform + Pitch + Slicer Pads + Loop Control)
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());

                    // OLED Status Display
                    let track_id = app.decks.now_playing[deck_idx];
                    let track = track_id.and_then(|id| app.library_db.get_track(id).ok().flatten());
                    let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions[deck_idx]).unwrap_or(0);

                    Frame::none()
                        .fill(theme.bg_inset)
                        .rounding(theme.radius_sm)
                        .inner_margin(Margin::symmetric(theme.space_md, theme.space_sm))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                if let Some(ref t) = track {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&t.title).strong().size(theme.type_body).color(theme.text_primary));
                                        ui.add_space(theme.space_sm);
                                        ui.label(RichText::new(format!("by {}", t.artist)).size(theme.type_caption).color(theme.text_secondary));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(format!("{:.1} BPM", t.metadata.bpm)).monospace().strong().color(deck_color).size(theme.type_body));
                                        });
                                    });
                                } else {
                                    ui.centered_and_justified(|ui| {
                                        ui.label(RichText::new("NO TRACK LOADED IN DECK").monospace().color(theme.text_disabled).size(theme.type_caption));
                                    });
                                }
                            });
                        });

                    ui.add_space(theme.space_sm);

                    // Waveform Display (Headless safe with fallback)
                    let (wf_rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 70.0), Sense::hover());
                    ui.painter().rect_filled(wf_rect, theme.radius_sm, theme.bg_inset);

                    if let Some(ref t) = track {
                        if let (Some(wgpu_mtx), Some(wf_mtx)) = (&app.wgpu_renderer, &app.deck_waveform_renderers[deck_idx]) {
                            let _wgpu = wgpu_mtx.lock();
                            let mut wf = wf_mtx.lock();
                            let zoom = 1.0;
                            let scroll = 0.0;
                            let color = deck_color.to_array().map(|v| v as f32 / 255.0);

                            wf.update_globals(&_wgpu.queue, scroll, zoom, color);
                            wf.update_from_mip_waveform(&_wgpu.queue, &t.metadata.mip_waveform, zoom, wf_rect.width() as u32, app.theme.accent.to_array().map(|v| v as f32 / 255.0));
                            nullherz_ui_hal::render::waveform_renderer::ui_paint_waveform(ui, wf_rect, wf_mtx.clone());
                        } else {
                            // Fallback rendering
                            ui.painter().text(wf_rect.center(), egui::Align2::CENTER_CENTER, format!("{} (NO GPU)", t.title), egui::FontId::monospace(theme.type_caption), theme.text_secondary);
                            ui.painter().line_segment(
                                [egui::pos2(wf_rect.min.x, wf_rect.center().y), egui::pos2(wf_rect.max.x, wf_rect.center().y)],
                                egui::Stroke::new(1.0, theme.border)
                            );
                        }

                        // Playhead line
                        let total_samples = t.metadata.total_samples.max(1);
                        let playhead_ratio = elapsed_samples as f32 / total_samples as f32;
                        let playhead_x = wf_rect.min.x + (playhead_ratio.clamp(0.0, 1.0) * wf_rect.width());
                        ui.painter().line_segment(
                            [egui::pos2(playhead_x, wf_rect.min.y), egui::pos2(playhead_x, wf_rect.max.y)],
                            egui::Stroke::new(2.0, deck_color)
                        );
                    }

                    ui.add_space(theme.space_sm);

                    // Transport and performance dashboard (Side-by-side controls)
                    ui.horizontal_top(|ui| {
                        // Play, pause, jump, loop sizes, and slicer pads
                        ui.vertical(|ui| {
                            ui.set_width(320.0);

                            // Transport row
                            ui.horizontal(|ui| {
                                if ui.button(RichText::new(egui_phosphor::regular::SKIP_BACK).size(theme.type_label)).clicked() {
                                    let node_name = match deck_idx {
                                        0 => "deck_a_sampler",
                                        1 => "deck_b_sampler",
                                        2 => "deck_c_sampler",
                                        3 => "deck_d_sampler",
                                        _ => "",
                                    };
                                    if let Some(node_idx) = app.get_node_id(node_name) {
                                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpByBeats { node_idx, beats: -4.0 }));
                                    }
                                }

                                let is_deck_playing = app.decks.deck_playing[deck_idx];
                                let play_btn = if is_deck_playing {
                                    egui::Button::new(RichText::new(egui_phosphor::regular::PAUSE).size(theme.type_heading)).fill(theme.accent_muted)
                                } else {
                                    egui::Button::new(RichText::new(egui_phosphor::regular::PLAY).size(theme.type_heading))
                                };

                                if ui.add(play_btn).clicked() {
                                    app.decks.deck_playing[deck_idx] = !is_deck_playing;
                                    if app.decks.deck_playing[deck_idx] {
                                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::PlayDeck { deck_id: deck_char }));
                                    } else {
                                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::StopDeck { deck_id: deck_char }));
                                    }
                                }

                                if ui.button(RichText::new(egui_phosphor::regular::SKIP_FORWARD).size(theme.type_label)).clicked() {
                                    let node_name = match deck_idx {
                                        0 => "deck_a_sampler",
                                        1 => "deck_b_sampler",
                                        2 => "deck_c_sampler",
                                        3 => "deck_d_sampler",
                                        _ => "",
                                    };
                                    if let Some(node_idx) = app.get_node_id(node_name) {
                                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpByBeats { node_idx, beats: 4.0 }));
                                    }
                                }

                                ui.separator();

                                // Slip mode toggle
                                let is_slip = app.mixer.channel_sync[deck_idx]; // Reuse sync array or represent slip
                                if ui.selectable_label(is_slip, "SLIP").clicked() {
                                    app.mixer.channel_sync[deck_idx] = !is_slip;
                                    let node_name = match deck_idx {
                                        0 => "deck_a_sampler",
                                        1 => "deck_b_sampler",
                                        2 => "deck_c_sampler",
                                        3 => "deck_d_sampler",
                                        _ => "",
                                    };
                                    if let Some(node_idx) = app.get_node_id(node_name) {
                                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::SetSlipMode { node_idx, enabled: !is_slip }));
                                    }
                                }
                            });

                            ui.add_space(theme.space_sm);

                            // Loop controls section
                            ui.strong("LOOP CONTROLS");
                            ui.horizontal(|ui| {
                                let loop_sizes = [1, 2, 4, 8, 16];
                                for sz in loop_sizes {
                                    if ui.button(format!("{}B", sz)).clicked() {
                                        let node_name = match deck_idx {
                                            0 => "deck_a_sampler",
                                            1 => "deck_b_sampler",
                                            2 => "deck_c_sampler",
                                            3 => "deck_d_sampler",
                                            _ => "",
                                        };
                                        let Some(node_idx) = app.get_node_id(node_name) else { return; };
                                        // Send loop configuration command
                                        let start = elapsed_samples;
                                        let sample_rate = telemetry.as_ref().map(|t| t.sample_rate).unwrap_or(44100.0);
                                        let bpm = track.as_ref().map(|t| t.metadata.bpm).unwrap_or(120.0);
                                        let beat_duration_samples = (60.0 / bpm * sample_rate) as u64;
                                        let end = start + (sz as u64 * beat_duration_samples);
                                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::SetLoop {
                                            node_idx,
                                            enabled: true,
                                            start_samples: start,
                                            end_samples: end,
                                        }));
                                    }
                                }
                                if ui.button(RichText::new("EXIT").color(theme.danger)).clicked() {
                                    let node_name = match deck_idx {
                                        0 => "deck_a_sampler",
                                        1 => "deck_b_sampler",
                                        2 => "deck_c_sampler",
                                        3 => "deck_d_sampler",
                                        _ => "",
                                    };
                                    if let Some(node_idx) = app.get_node_id(node_name) {
                                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::SetLoop {
                                            node_idx,
                                            enabled: false,
                                            start_samples: 0,
                                            end_samples: 0,
                                        }));
                                    }
                                }
                            });
                        });

                        ui.separator();

                        // Performance slice pads (TriggerSlice with dynamic beat-grid quantization)
                        ui.vertical(|ui| {
                            ui.strong("REAL-TIME TRACK SLICER");
                            ui.add_space(theme.space_xs);
                            egui::Grid::new("slicer_pads_grid").spacing([theme.space_xs, theme.space_xs]).show(ui, |ui| {
                                for r in 0..2 {
                                    for c in 0..4 {
                                        let pad_idx = r * 4 + c;
                                        let btn = egui::Button::new(RichText::new(format!("SL {}", pad_idx + 1)).strong().size(theme.type_caption))
                                            .min_size(Vec2::new(42.0, 24.0))
                                            .fill(theme.bg_surface);
                                        if ui.add(btn).clicked() {
                                            let node_name = match deck_idx {
                                                0 => "deck_a_sampler",
                                                1 => "deck_b_sampler",
                                                2 => "deck_c_sampler",
                                                3 => "deck_d_sampler",
                                                _ => "",
                                            };
                                            let Some(node_idx) = app.get_node_id(node_name) else { return; };

                                            // Quantized micro-jump calculation
                                            let current_beat = telemetry.as_ref().map(|t| t.get_interpolated_beat_position()).unwrap_or(0.0);
                                            let current_bar = (current_beat / 4.0).floor() * 4.0;
                                            let target_beat = current_bar + (pad_idx as f64 * 0.5); // 1/2 beat slice offsets
                                            let mut beats_to_jump = target_beat - current_beat;

                                            if app.mixer.quantize_enabled {
                                                // Quantize jump to the nearest 1/8th of a beat
                                                beats_to_jump = (beats_to_jump * 8.0).round() / 8.0;
                                            }

                                            // Dispatch the micro-jump and slice triggering to the execution plane
                                            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpByBeats {
                                                node_idx,
                                                beats: beats_to_jump as f32,
                                            }));
                                            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::TriggerSlice {
                                                node_idx,
                                                slice_idx: pad_idx as u32,
                                            }));
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                        });
                    });
                });
            });
        });

    ui.add_space(theme.space_md);
    ui.separator();
    ui.add_space(theme.space_sm);

    // Modern Library Browser with load buttons targeting focused deck
    ui.horizontal(|ui| {
        ui.heading(RichText::new("Precision Library Browser").size(theme.type_heading));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.text_edit_singleline(&mut app.library.search_query);
            ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
        });
    });
    ui.add_space(theme.space_sm);

    ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("library_grid").num_columns(5).spacing([theme.space_lg, theme.space_sm]).striped(true).show(ui, |ui| {
            ui.label(RichText::new("TITLE").strong().size(theme.type_label));
            ui.label(RichText::new("ARTIST").strong().size(theme.type_label));
            ui.label(RichText::new("BPM").strong().size(theme.type_label));
            ui.label(RichText::new("KEY").strong().size(theme.type_label));
            ui.label("");
            ui.end_row();

            let query = app.library.search_query.to_lowercase();
            for track in &app.library.cached_library {
                if !query.is_empty() && !track.title.to_lowercase().contains(&query) && !track.artist.to_lowercase().contains(&query) {
                    continue;
                }

                ui.label(RichText::new(&track.title).size(theme.type_body));
                ui.label(RichText::new(&track.artist).size(theme.type_body));
                ui.label(RichText::new(format!("{:.1}", track.metadata.bpm)).size(theme.type_body));
                ui.label(RichText::new(format!("{}", track.metadata.root_key.unwrap_or(0.0))).size(theme.type_body));

                ui.horizontal(|ui| {
                    if ui.button(RichText::new(format!("LOAD TO DECK {}", deck_char)).color(deck_color).strong().size(theme.type_caption)).clicked() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::LoadTrackToDeck {
                            deck_id: deck_char,
                            sample_id: track.id,
                        }));
                        app.decks.now_playing[deck_idx] = Some(track.id);
                    }
                    if ui.button(RichText::new("QUEUE").size(theme.type_caption)).clicked() {
                        app.library.playlist_queue.push_back(track.id);
                    }
                });
                ui.end_row();
            }
        });

        ui.add_space(theme.space_lg);
        ui.heading(RichText::new("Playlist Queue").size(theme.type_heading));
        let mut to_remove = None;
        for (idx, &track_id) in app.library.playlist_queue.iter().enumerate() {
            if let Ok(Some(track)) = app.library_db.get_track(track_id) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}. {} - {}", idx + 1, track.artist, track.title)).size(theme.type_body));
                    if ui.button(egui_phosphor::regular::X).clicked() { to_remove = Some(idx); }
                });
            }
        }
        if let Some(idx) = to_remove { app.library.playlist_queue.remove(idx); }
    });
}
