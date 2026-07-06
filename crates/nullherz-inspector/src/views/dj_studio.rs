use egui::{Ui, Color32, RichText, Vec2, Frame, Margin, Rounding, Stroke};
use crate::{InspectorApp, widgets};
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ScrollArea::vertical().show(ui, |ui| {
        render_header(ui, telemetry);
        ui.add_space(15.0);

        // 4-Deck Modular Grid
        ui.columns(2, |cols| {
            render_deck_card(app, &mut cols[0], 0, telemetry);
            render_deck_card(app, &mut cols[1], 1, telemetry);
        });

        ui.add_space(10.0);

        ui.columns(2, |cols| {
            render_deck_card(app, &mut cols[0], 2, telemetry);
            render_deck_card(app, &mut cols[1], 3, telemetry);
        });

        ui.add_space(20.0);
        render_master_section(app, ui, telemetry);
    });
}

fn render_header(ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        Frame::none()
            .fill(Color32::from_rgb(20, 20, 25))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::same(8.0))
            .show(ui, |ui| {
                ui.heading(RichText::new("LIVE CONSOLE").strong().color(Color32::WHITE).size(18.0));
                ui.add_space(20.0);
                if let Some(t) = telemetry {
                    ui.label(RichText::new(format!("{:.1}", t.bpm)).monospace().color(Color32::from_rgb(0, 255, 200)).size(16.0));
                    ui.label(RichText::new("BPM").small().color(Color32::GRAY));
                }
            });
    });
}

fn render_deck_card(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    let deck_color = InspectorApp::deck_color(i);
    let is_focused = app.focused_deck == i;

    let frame = Frame::group(ui.style())
        .fill(Color32::from_rgb(15, 15, 20))
        .stroke(Stroke::new(1.0, if is_focused { deck_color } else { Color32::from_gray(30) }))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::same(10.0));

    frame.show(ui, |ui| {
        ui.vertical(|ui| {
            // --- DECK HEADER ---
            ui.horizontal(|ui| {
                let deck_id_label = (b'A' + i as u8) as char;
                if ui.selectable_label(is_focused, RichText::new(format!("DECK {}", deck_id_label)).strong().size(14.0).color(deck_color)).clicked() {
                    app.focused_deck = i;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.selectable_label(app.channel_sync[i], "SYNC").clicked() {
                        app.channel_sync[i] = !app.channel_sync[i];
                        // Sync Logic...
                    }
                });
            });

            ui.add_space(8.0);

            // --- WAVEFORM ZONE ---
            render_waveform_zone(app, ui, i, telemetry, deck_color);

            ui.add_space(12.0);

            ui.horizontal_top(|ui| {
                // --- EXECUTION ZONE (LEFT) ---
                ui.vertical(|ui| {
                    ui.set_min_width(50.0);
                    let deck_id = (b'A' + i as u8) as char;
                    if ui.add_sized([45.0, 40.0], egui::Button::new(RichText::new("▶").size(18.0)).fill(Color32::from_gray(35))).clicked() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::PlayDeck { deck_id }));
                    }
                    ui.add_space(6.0);
                    if ui.add_sized([45.0, 40.0], egui::Button::new(RichText::new("⏸").size(18.0)).fill(Color32::from_gray(35))).clicked() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::StopDeck { deck_id }));
                    }
                });

                ui.add_space(10.0);

                // --- PERFORMANCE ZONE (MIDDLE-LEFT) ---
                ui.vertical(|ui| {
                    ui.label(RichText::new("PERFORM").small().color(Color32::from_gray(100)));
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(2.0);
                        for j in 0..8 {
                            if ui.add_sized([28.0, 24.0], egui::Button::new(format!("{}", j + 1)).fill(Color32::from_gray(30))).clicked() {
                                let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpToHotCue {
                                    node_idx: (i as u32 * 4),
                                    cue_idx: j as u32,
                                }));
                            }
                        }
                    });
                });

                ui.add_space(10.0);

                // --- PERSONALITY ZONE (MIDDLE-RIGHT) ---
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("DNA").small().color(Color32::from_gray(100)));
                        ui.checkbox(&mut app.personality_macro_mode, "🔗");
                    });

                    let traits = [
                        ("MET", 0, "metallic"),
                        ("ORG", 1, "organic"),
                        ("WRM", 2, "warm"),
                        ("AGG", 3, "aggressive"),
                    ];

                    for (label, idx, feature) in traits {
                        ui.horizontal(|ui| {
                            ui.add_sized([25.0, 12.0], egui::Label::new(RichText::new(label).size(8.0)));

                            let val = match idx {
                                0 => &mut app.channel_personality_metallic[i],
                                1 => &mut app.channel_personality_organic[i],
                                2 => &mut app.channel_personality_warm[i],
                                _ => &mut app.channel_personality_aggressive[i],
                            };

                            if ui.add(egui::Slider::new(val, 0.0..=1.0).show_value(false).trailing_fill(true)).changed() {
                                let strength = *val;
                                emit_personality_mutation(app, i, idx, feature, strength);
                            }
                        });
                    }
                });

                ui.add_space(10.0);

                // --- MIXER ZONE (RIGHT) ---
                ui.vertical(|ui| {
                    ui.set_min_width(80.0);
                    let deck_id = (b'A' + i as u8) as char;

                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            if widgets::render_knob(ui, &mut app.channel_eq_high[i], 0.0..=2.0, "HI", deck_color).changed() {
                                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqHigh, app.channel_eq_high[i]);
                            }
                            if widgets::render_knob(ui, &mut app.channel_eq_mid[i], 0.0..=2.0, "MID", deck_color).changed() {
                                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqMid, app.channel_eq_mid[i]);
                            }
                            if widgets::render_knob(ui, &mut app.channel_eq_low[i], 0.0..=2.0, "LOW", deck_color).changed() {
                                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqLow, app.channel_eq_low[i]);
                            }
                        });

                        ui.add_space(5.0);

                        ui.horizontal(|ui| {
                            let peak = app.damped_peaks[i];
                            widgets::render_vu_meter(ui, peak, app.channel_peak_hold[i], deck_color, 120.0);
                            if widgets::render_fader(ui, &mut app.channel_faders[i], 0.0..=1.0, deck_color, 120.0, 16.0).changed() {
                                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::Gain, app.channel_faders[i]);
                            }
                        });
                    });
                });
            });
        });
    });
}

fn render_waveform_zone(app: &InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>, deck_color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 60.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(10, 10, 15));

    if let Some(wf_lock) = &app.waveform_renderer {
        if let Some(track_id) = app.now_playing[i] {
            let mut wf = wf_lock.lock().unwrap();
            let zoom = 1.0;
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

            let title = track.as_ref().map(|t| t.title.as_str()).unwrap_or("LOADING...");
            ui.painter().text(rect.left_top() + Vec2::new(5.0, 5.0), egui::Align2::LEFT_TOP, title, egui::FontId::monospace(10.0), deck_color.gamma_multiply(0.8));

            // Render playhead
            let playhead_x = rect.min.x + (telemetry.as_ref().map(|t| (t.beat_position % 4.0) / 4.0).unwrap_or(0.0) as f32 * rect.width());
            ui.painter().line_segment([egui::pos2(playhead_x, rect.min.y), egui::pos2(playhead_x, rect.max.y)], egui::Stroke::new(1.0, Color32::WHITE));
        } else {
            // Enhanced EMPTY DECK visualization
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "EMPTY DECK", egui::FontId::monospace(12.0), Color32::from_gray(60));
            // Render a dashed border for the empty zone
            ui.painter().rect_stroke(rect.shrink(2.0), 2.0, Stroke::new(1.0, Color32::from_gray(30)));
        }
    }
}

fn render_master_section(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    Frame::group(ui.style())
        .fill(Color32::from_rgb(20, 20, 25))
        .inner_margin(Margin::same(15.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("MASTER").strong());
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        let peak = (app.damped_master_peaks[0] + app.damped_master_peaks[1]) * 0.5;
                        widgets::render_vu_meter(ui, peak, app.master_peak_hold, Color32::WHITE, 140.0);
                        widgets::render_fader(ui, &mut app.master_gain, 0.0..=1.5, Color32::WHITE, 140.0, 20.0);
                    });
                });

                ui.add_space(30.0);

                ui.vertical(|ui| {
                    ui.set_min_width(ui.available_width() - 50.0);
                    ui.centered_and_justified(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("CROSSFADER").small().color(Color32::GRAY));
                            if widgets::render_horizontal_fader(ui, &mut app.crossfader_pos, 0.0..=1.0, Color32::WHITE, ui.available_width(), 35.0).changed() {
                                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: 20,
                                    param_id: 0,
                                    value: app.crossfader_pos,
                                    ramp_duration_samples: 0,
                                }));
                            }
                        });
                    });
                });
            });
        });
}

fn emit_personality_mutation(app: &mut InspectorApp, deck_idx: usize, trait_idx: usize, feature: &str, strength: f32) {
    let mut targets = vec![];
    if app.personality_macro_mode {
        for i in 0..4 {
            if let Some(id) = app.now_playing[i] {
                targets.push(id);
                match trait_idx {
                    0 => app.channel_personality_metallic[i] = strength,
                    1 => app.channel_personality_organic[i] = strength,
                    2 => app.channel_personality_warm[i] = strength,
                    _ => app.channel_personality_aggressive[i] = strength,
                }
            }
        }
    } else if let Some(id) = app.now_playing[deck_idx] {
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

fn send_deck_param(app: &InspectorApp, deck_id: char, param_type: nullherz_traits::DeckParamType, value: f32) {
    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetDeckParam {
        deck_id,
        param_type,
        value,
    }));
}

use egui::ScrollArea;
