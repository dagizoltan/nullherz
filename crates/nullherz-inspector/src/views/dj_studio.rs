use egui::{Ui, Color32, RichText, Vec2};
use crate::{InspectorApp, widgets};
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("LIVE STUDIO").strong().color(Color32::WHITE));
            ui.add_space(20.0);
            if let Some(t) = telemetry {
                 ui.label(RichText::new(format!("{:.1} BPM", t.bpm)).monospace().color(Color32::from_rgb(0, 255, 200)));
            }
        });

        ui.add_space(10.0);

        // 4-Deck Grid
        ui.columns(2, |cols| {
            render_deck(app, &mut cols[0], 0, telemetry);
            render_deck(app, &mut cols[1], 1, telemetry);
        });

        ui.add_space(10.0);

        ui.columns(2, |cols| {
            render_deck(app, &mut cols[0], 2, telemetry);
            render_deck(app, &mut cols[1], 3, telemetry);
        });

        ui.add_space(20.0);

        // Master/Mixer Section at the bottom
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("CROSSFADER");
                if widgets::render_horizontal_fader(ui, &mut app.crossfader_pos, 0.0..=1.0, Color32::WHITE, ui.available_width() - 100.0, 30.0).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                        target_id: 20, // Crossfader default ID
                        param_id: 0,
                        value: app.crossfader_pos,
                        ramp_duration_samples: 0,
                    }));
                }
            });
        });
    });
}

fn render_deck(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    let deck_color = InspectorApp::deck_color(i);
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("DECK {}", i + 1)).strong().color(deck_color));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.selectable_label(app.channel_sync[i], "SYNC").clicked() {
                    app.channel_sync[i] = !app.channel_sync[i];
                    if app.channel_sync[i] {
                         let target_deck = (b'A' + i as u8) as char;
                         let source_deck = app.master_deck.map(|idx| (b'A' + idx as u8) as char).unwrap_or('A');
                         let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::SyncDecks { source_deck, target_deck }));
                    }
                }
            });
        });

        ui.add_space(5.0);

        // Compact Modern Waveform
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 50.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.0, Color32::from_rgb(10, 10, 15));

        if let Some(wf_lock) = &app.waveform_renderer {
             if let Some(track_id) = app.now_playing[i] {
                 // High-fidelity GPU waveform rendering
                 let mut wf = wf_lock.lock().unwrap();
                 let zoom = 1.0; // Standard view
                 let scroll = 0.0;
                 let color = deck_color.to_array().map(|v| v as f32 / 255.0);

                 let track = app.library_db.get_track(track_id).ok().flatten();

                 if let Some(ref t) = track {
                     if let Some(wgpu) = &app.wgpu_renderer {
                         let wgpu = wgpu.lock().unwrap();
                         wf.update_globals(&wgpu.queue, scroll, zoom, color);
                         wf.update_from_mip_waveform(&wgpu.queue, &t.metadata.mip_waveform, zoom, rect.width() as u32);
                     }
                 }

                 let title = track.as_ref().map(|t| t.title.as_str()).unwrap_or("UNKNOWN");
                 ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, title, egui::FontId::monospace(9.0), deck_color.gamma_multiply(0.5));

                 // Render playhead (Modern thin line)
                 let playhead_x = rect.min.x + (telemetry.as_ref().map(|t| (t.beat_position % 4.0) / 4.0).unwrap_or(0.0) as f32 * rect.width());
                 ui.painter().line_segment([egui::pos2(playhead_x, rect.min.y), egui::pos2(playhead_x, rect.max.y)], egui::Stroke::new(1.0, Color32::WHITE));
             } else {
                 ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "EMPTY DECK", egui::FontId::monospace(9.0), Color32::from_gray(40));
             }
        } else {
             ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "WAVEFORM OFFLINE", egui::FontId::monospace(8.0), Color32::from_gray(30));
        }

        ui.horizontal(|ui| {
            // High-Density Industrial Channel Strip
            ui.vertical(|ui| {
                ui.set_min_width(40.0);
                let deck_id = (b'A' + i as u8) as char;
                if ui.add_sized([35.0, 30.0], egui::Button::new(RichText::new("▶").size(14.0)).fill(Color32::from_gray(30))).clicked() {
                     let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::PlayDeck { deck_id }));
                }
                ui.add_space(4.0);
                if ui.add_sized([35.0, 30.0], egui::Button::new(RichText::new("⏸").size(14.0)).fill(Color32::from_gray(30))).clicked() {
                     let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::StopDeck { deck_id }));
                }
            });

            ui.add_space(10.0);

            // Layer 6: Semantic Personality Shaping
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("PERSONALity").small().color(Color32::from_gray(120)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                         ui.checkbox(&mut app.personality_macro_mode, "🔗").on_hover_text("Global Macro Mode: Apply to all decks");
                    });
                });
                ui.add_space(2.0);

                let traits = [
                    ("MET", 0, "metallic"),
                    ("ORG", 1, "organic"),
                    ("WRM", 2, "warm"),
                    ("AGG", 3, "aggressive"),
                ];

                for (label, idx, feature) in traits {
                    let val = match idx {
                        0 => &mut app.channel_personality_metallic[i],
                        1 => &mut app.channel_personality_organic[i],
                        2 => &mut app.channel_personality_warm[i],
                        _ => &mut app.channel_personality_aggressive[i],
                    };

                    if ui.add(egui::Slider::new(val, 0.0..=1.0).text(label).show_value(false)).changed() {
                        let strength = *val;
                        let mut targets = vec![];
                        if app.personality_macro_mode {
                            for deck_idx in 0..4 {
                                if let Some(id) = app.now_playing[deck_idx] {
                                    targets.push(id);
                                    // Update macro-linked values visually
                                    match idx {
                                        0 => app.channel_personality_metallic[deck_idx] = strength,
                                        1 => app.channel_personality_organic[deck_idx] = strength,
                                        2 => app.channel_personality_warm[deck_idx] = strength,
                                        _ => app.channel_personality_aggressive[deck_idx] = strength,
                                    }
                                }
                            }
                        } else if let Some(id) = app.now_playing[i] {
                            targets.push(id);
                        }

                        for track_id in targets {
                            let mut name = [0u8; 32];
                            let bytes = feature.as_bytes();
                            name[..bytes.len()].copy_from_slice(bytes);

                            let cmd = nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::ApplyFeatureMutation {
                                target_id: track_id,
                                feature_name: name,
                                strength,
                            });
                            let _ = app.command_sender.send(cmd);
                        }
                    }
                }
            });

            ui.add_space(12.0);

            // Mixer Strip for Deck (EQ Stack)
            ui.vertical(|ui| {
                let deck_id = (b'A' + i as u8) as char;
                if widgets::render_knob(ui, &mut app.channel_eq_high[i], 0.0..=2.0, "HI", deck_color).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetDeckParam {
                        deck_id,
                        param_type: nullherz_traits::DeckParamType::EqHigh,
                        value: app.channel_eq_high[i],
                    }));
                }
                if widgets::render_knob(ui, &mut app.channel_eq_mid[i], 0.0..=2.0, "MID", deck_color).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetDeckParam {
                        deck_id,
                        param_type: nullherz_traits::DeckParamType::EqMid,
                        value: app.channel_eq_mid[i],
                    }));
                }
                if widgets::render_knob(ui, &mut app.channel_eq_low[i], 0.0..=2.0, "LOW", deck_color).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetDeckParam {
                        deck_id,
                        param_type: nullherz_traits::DeckParamType::EqLow,
                        value: app.channel_eq_low[i],
                    }));
                }
                if widgets::render_knob(ui, &mut app.channel_trims[i], 0.0..=2.0, "FLT", deck_color).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetDeckParam {
                        deck_id,
                        param_type: nullherz_traits::DeckParamType::Filter,
                        value: app.channel_trims[i],
                    }));
                }
            });

            ui.add_space(8.0);

            // Fader & VU (Integrated Aesthetic)
            ui.horizontal(|ui| {
                let peak = telemetry.as_ref().map(|t| t.peak_levels[i]).unwrap_or(0.0);
                widgets::render_vu_meter(ui, peak, app.channel_peak_hold[i], deck_color, 140.0);
                if widgets::render_fader(ui, &mut app.channel_faders[i], 0.0..=1.0, deck_color, 140.0, 16.0).changed() {
                    let deck_id = (b'A' + i as u8) as char;
                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetDeckParam {
                        deck_id,
                        param_type: nullherz_traits::DeckParamType::Gain,
                        value: app.channel_faders[i],
                    }));
                }
            });
        });
    });
}
