use egui::{Ui, Color32, RichText, FontId, Align2, vec2, Rect, Stroke, Sense, Frame, Layout, Align};
use crate::{InspectorApp, widgets};
use audio_core::Telemetry;

const PANEL_ROUNDING: f32 = 4.0;
const INNER_MARGIN: f32 = 8.0;
const ITEM_SPACING: f32 = 10.0;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let total_w = ui.available_width();

    ui.spacing_mut().item_spacing = vec2(0.0, ITEM_SPACING);

    ui.vertical(|ui| {
        ui.add_space(INNER_MARGIN);

        // LAYER 1: MASTER MIX OSCILLOSCOPE (Now integrated at the top)
        render_rolling_waveform(app, ui, telemetry, total_w);

        // LAYER 2: GLOBAL ALIGNMENT TIMELINE
        render_signal_stack(app, ui, telemetry, total_w);

        // LAYER 3: DECK CONTROLS
        render_deck_controls_stack(app, ui, telemetry);

        // LAYER 4: CENTRAL MIXER
        render_central_mixer(app, ui, telemetry, total_w, 420.0);
    });
}

fn render_rolling_waveform(app: &InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 160.0), Sense::hover());
    ui.painter().rect_filled(rect, PANEL_ROUNDING, Color32::from_rgb(5, 5, 6));
    ui.painter().rect_stroke(rect, PANEL_ROUNDING, Stroke::new(1.0, Color32::from_gray(20)));

    let w = rect.width();
    let playhead_x = rect.center().x;

    // High-density background grid
    for i in 0..32 {
        let x = rect.min.x + (i as f32 * (w / 32.0));
        ui.painter().vline(x, rect.y_range(), Stroke::new(0.5, Color32::from_rgba_unmultiplied(255, 255, 255, 5)));
    }

    let deck_idx = app.selected_deck;
    let deck_color = InspectorApp::deck_color(deck_idx);

    if let Some(sample) = app.sample_registry.get(deck_idx as u64 * 4) {
        let peaks = &sample.metadata.peaks;
        let total_samples = sample.buffer.len() as f32;

        if !peaks.is_empty() && total_samples > 0.0 {
            let sample_counter = telemetry.as_ref().map_or(0, |t| t.sample_counter);
            let current_pos_norm = (sample_counter as f32 / total_samples).clamp(0.0, 1.0);

            let view_range = 0.05; // Show 5% of the track
            let dw = rect.width();
            let dh = rect.height();

            let mut points = Vec::new();
            for x in 0..dw as usize {
                let rel_x = (x as f32 - dw / 2.0) / dw;
                let track_norm = (current_pos_norm + rel_x * view_range).clamp(0.0, 1.0);
                let p_idx = (track_norm * (peaks.len() - 1) as f32) as usize;
                let val = peaks[p_idx];

                let y_off = val * (dh * 0.4);
                points.push(egui::pos2(rect.min.x + x as f32, rect.center().y - y_off));
                points.push(egui::pos2(rect.min.x + x as f32, rect.center().y + y_off));
            }

            // Glow layers for the rolling waveform
            ui.painter().add(egui::Shape::line(points.clone(), Stroke::new(3.0, deck_color.linear_multiply(0.2))));
            ui.painter().add(egui::Shape::line(points, Stroke::new(1.2, deck_color)));

            // Playhead indicator
            ui.painter().vline(playhead_x, rect.y_range(), Stroke::new(1.0, Color32::WHITE.linear_multiply(0.5)));

            let deck_label = format!("DECK {} WAVEFORM", (b'A' + deck_idx as u8) as char);
            ui.painter().text(rect.min + vec2(10.0, 10.0), Align2::LEFT_TOP, deck_label, FontId::proportional(11.0), Color32::from_gray(120));
        } else {
             ui.painter().text(rect.center(), Align2::CENTER_CENTER, "ANALYZING WAVEFORM...", FontId::proportional(14.0), Color32::from_gray(80));
        }
    } else {
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, "NO TRACK LOADED IN SELECTED DECK", FontId::proportional(14.0), Color32::from_gray(60));
    }
}

fn render_signal_stack(app: &mut InspectorApp, ui: &mut Ui, _telemetry: &Option<Telemetry>, width: f32) {
    let total_h = 320.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, total_h), Sense::hover());
    ui.painter().rect_filled(rect, PANEL_ROUNDING, Color32::from_rgb(5, 5, 6));
    ui.painter().rect_stroke(rect, PANEL_ROUNDING, Stroke::new(1.0, Color32::from_gray(20)));

    let time = ui.input(|i| i.time);
    let deck_h = (total_h - 10.0) / 4.0;
    let playhead_x = rect.center().x;

    for i in 0..4 {
        let deck_color = InspectorApp::deck_color(i);
        let deck_rect = Rect::from_min_size(
            rect.min + vec2(5.0, 5.0 + i as f32 * deck_h),
            vec2(width - 10.0, deck_h - 2.0)
        );

        ui.painter().rect_filled(deck_rect, 2.0, Color32::from_rgb(10, 10, 12));

        // Shared playhead vertical line (visual only within deck)
        ui.painter().vline(playhead_x, deck_rect.y_range(), Stroke::new(1.0, Color32::from_gray(30)));

        if let Some(sample) = app.sample_registry.get(i as u64 * 4) {
            let peaks = &sample.metadata.peaks;
            let view_range = 0.05; // 5% of track
            let play_head_norm = (time as f32 % 30.0) / 30.0; // Simulated playhead

            // 1. Phrase Structure (Top 20% of deck height)
            let phrase_h = deck_rect.height() * 0.2;
            let phrase_rect = Rect::from_min_size(deck_rect.min, vec2(deck_rect.width(), phrase_h));
            let block_w = deck_rect.width() / 16.0;
            for b in 0..16 {
                let bx = phrase_rect.min.x + b as f32 * block_w;
                let b_rect = Rect::from_min_size(egui::pos2(bx, phrase_rect.min.y), vec2(block_w - 1.0, phrase_h - 1.0));
                let is_active = (b as f32 / 16.0) < play_head_norm;
                let fill = if is_active { deck_color.linear_multiply(0.3) } else { Color32::from_gray(20) };
                ui.painter().rect_filled(b_rect, 1.0, fill);
            }

            // 2. Beat Grid (Middle 20%)
            let grid_h = deck_rect.height() * 0.2;
            let grid_rect = Rect::from_min_size(deck_rect.min + vec2(0.0, phrase_h), vec2(deck_rect.width(), grid_h));
            let total_samples = sample.buffer.len() as f32;
            if total_samples > 0.0 {
                for &pos in sample.metadata.transients.iter() {
                    let rel_pos = pos as f32 / total_samples;
                    let x_dist = rel_pos - play_head_norm;
                    if x_dist.abs() < view_range / 2.0 {
                        let x_off = (x_dist / view_range) * deck_rect.width();
                        let tx = playhead_x + x_off;
                        if grid_rect.x_range().contains(tx) {
                            ui.painter().circle_filled(egui::pos2(tx, grid_rect.center().y), 2.0, deck_color);
                        }
                    }
                }
            }

            // 3. Waveform Detail (Bottom 60%)
            let wave_h = deck_rect.height() * 0.6;
            let wave_rect = Rect::from_min_size(deck_rect.min + vec2(0.0, phrase_h + grid_h), vec2(deck_rect.width(), wave_h));

            if !peaks.is_empty() {
                let mut points = Vec::new();
                let dw = wave_rect.width();
                let dh = wave_rect.height();
                for x in 0..dw as usize {
                    let rel_x = (x as f32 - dw / 2.0) / dw;
                    let track_norm = (play_head_norm + rel_x * view_range).clamp(0.0, 1.0);
                    let p_idx = (track_norm * peaks.len() as f32) as usize;
                    let val = peaks[p_idx.min(peaks.len() - 1)];
                    let y_off = val * (dh / 2.5);
                    points.push(egui::pos2(wave_rect.min.x + x as f32, wave_rect.center().y - y_off));
                    points.push(egui::pos2(wave_rect.min.x + x as f32, wave_rect.center().y + y_off));
                }
                ui.painter().add(egui::Shape::line(points, Stroke::new(1.0, deck_color.additive())));

                // Restore Hot Cues
                for (cue_idx, cue) in sample.metadata.hot_cues.iter().enumerate() {
                    if let Some(pos) = cue {
                        let rel_pos = *pos as f32 / total_samples;
                        let x_dist = rel_pos - play_head_norm;
                        if x_dist.abs() < view_range / 2.0 {
                            let x_off = (x_dist / view_range) * wave_rect.width();
                            let tx = playhead_x + x_off;
                            if wave_rect.x_range().contains(tx) {
                                ui.painter().vline(tx, wave_rect.y_range(), Stroke::new(1.5, Color32::YELLOW));
                                ui.painter().text(egui::pos2(tx, wave_rect.min.y + 2.0), Align2::LEFT_TOP, format!("{}", cue_idx+1), FontId::proportional(8.0), Color32::WHITE);
                            }
                        }
                    }
                }
            }
        } else {
            // Empty state label
            ui.painter().text(deck_rect.center(), Align2::CENTER_CENTER, "NO TRACK LOADED", FontId::proportional(10.0), Color32::from_gray(40));
        }
    }

    // Global Playhead Line across all decks
    ui.painter().vline(playhead_x, rect.y_range(), Stroke::new(1.5, Color32::WHITE.linear_multiply(0.5)));
    ui.painter().text(rect.min + vec2(10.0, 10.0), Align2::LEFT_TOP, "GLOBAL ALIGNMENT TIMELINE", FontId::proportional(11.0), Color32::from_gray(120));
}

fn render_deck_controls_row(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    let color = InspectorApp::deck_color(i);
    let is_selected = app.selected_deck == i;

    Frame::none()
        .fill(if is_selected { color.linear_multiply(0.05) } else { Color32::from_rgb(15, 15, 18) })
        .rounding(PANEL_ROUNDING)
        .stroke(Stroke::new(if is_selected { 2.0 } else { 1.0 }, if is_selected { color } else { Color32::from_gray(30) }))
        .inner_margin(INNER_MARGIN)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // 1. Deck Label & Selection & Master Toggle
                ui.vertical(|ui| {
                    let (rect, res) = ui.allocate_exact_size(vec2(40.0, 30.0), Sense::click());
                    ui.painter().rect_filled(rect, 2.0, if is_selected { color } else { Color32::from_gray(25) });
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, format!("{}", (b'A' + i as u8) as char), FontId::proportional(16.0), if is_selected { Color32::BLACK } else { color });
                    if res.clicked() { app.selected_deck = i; }

                    let is_master = app.master_deck == Some(i);
                    let m_color = if is_master { color } else { Color32::from_gray(40) };
                    if ui.add(egui::Button::new(RichText::new("MST").small().strong().color(m_color)).frame(true)).clicked() {
                         if is_master { app.master_deck = None; } else { app.master_deck = Some(i); }
                    }
                });

                ui.add_space(8.0);

                // 2. Track Info & Time
                ui.vertical(|ui| {
                    ui.set_width(200.0);
                    if let Some(ref title) = app.now_playing[i] {
                        ui.label(RichText::new(title).color(color).strong());
                    } else {
                        ui.label(RichText::new("EMPTY").color(Color32::from_gray(60)).strong());
                    }

                    ui.horizontal(|ui| {
                        if let Some(sample) = app.sample_registry.get(i as u64 * 4) {
                            ui.label(RichText::new(format!("{:.1} BPM", sample.metadata.bpm)).small().color(color));
                            if let Some(key) = sample.metadata.root_key {
                                let notes = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
                                ui.label(RichText::new(format!("K:{}", notes[(key.round() as usize) % 12])).small().color(color));
                            }
                        }

                        // Restore Track Timing
                        let total_sec = 324.0;
                        let elapsed = (ui.input(|i| i.time) % total_sec as f64) as f32;
                        let remaining = total_sec - elapsed;
                        let mins = (remaining / 60.0) as i32;
                        let secs = (remaining % 60.0) as i32;
                        ui.label(RichText::new(format!("-{:02}:{:02}", mins, secs)).color(color).monospace().size(11.0).strong());
                    });
                });

                ui.add_space(10.0);

                // 3. Transport
                ui.horizontal(|ui| {
                    let cue_color = if app.channel_cue[i] { Color32::from_rgb(255, 150, 0) } else { Color32::from_gray(40) };
                    if ui.add(egui::Button::new(RichText::new("CUE").color(cue_color).strong()).min_size(vec2(45.0, 28.0))).clicked() {
                        app.channel_cue[i] = !app.channel_cue[i];
                    }

                    let play_color = Color32::from_rgb(0, 255, 100);
                    if ui.add(egui::Button::new(RichText::new("PLAY").color(play_color).strong()).min_size(vec2(50.0, 28.0))).clicked() {
                        // Play toggle
                    }

                    let s_color = if app.channel_sync[i] { color } else { Color32::from_gray(40) };
                    if ui.add(egui::Button::new(RichText::new("SYNC").color(s_color).strong().size(10.0)).min_size(vec2(40.0, 28.0))).clicked() {
                        app.channel_sync[i] = !app.channel_sync[i];
                        let _ = app.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: (i as u64 * 4),
                            param_id: 2,
                            value: if app.channel_sync[i] { 1.0 } else { 0.0 },
                            ramp_duration_samples: 0,
                        });
                    }
                });

                ui.add_space(15.0);

                // 4. Pitch
                ui.horizontal(|ui| {
                    ui.set_width(120.0);
                    let range = 0.92..=1.08;
                    let p_res = widgets::render_horizontal_fader(ui, &mut app.pitch_bend[i], range, color, 80.0, 16.0);
                    if p_res.changed() {
                        let _ = app.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: (i as u64 * 4),
                            param_id: 1,
                            value: app.pitch_bend[i],
                            ramp_duration_samples: 128,
                        });
                    }
                    let pct = (app.pitch_bend[i] - 1.0) * 100.0;
                    ui.label(RichText::new(format!("{:+.1}%", pct)).size(9.0).monospace().color(color));
                });

                // 5. VU Mini
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1].min(1.2));
                    widgets::render_vu_meter(ui, peak, peak, Color32::from_rgb(0, 255, 180), 28.0);
                });
            });
        });
}

fn render_deck_controls_stack(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
        for i in 0..4 {
            render_deck_controls_row(app, ui, i, telemetry);
        }
    });
}

fn render_central_mixer(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>, main_w: f32, height: f32) {
    let rect = ui.allocate_exact_size(vec2(main_w, height), Sense::hover()).0;
    ui.painter().rect_filled(rect, PANEL_ROUNDING, Color32::from_rgb(15, 15, 18));
    ui.painter().rect_stroke(rect, PANEL_ROUNDING, Stroke::new(1.0, Color32::from_gray(30)));

    let inner_rect = rect.shrink2(vec2(INNER_MARGIN, INNER_MARGIN / 2.0));
    ui.allocate_ui_at_rect(inner_rect, |ui| {
        ui.vertical(|ui| {
            // 1. MASTER CONTROLS STANDALONE ROW (Aligned with Channel Columns)
            ui.add_space(4.0);
            ui.columns(4, |cols| {
                let labels = ["BOOTH", "REC", "MST"];
                let colors = [Color32::from_rgb(0, 180, 255), Color32::from_rgb(255, 50, 150), Color32::from_rgb(0, 255, 180)];

                for i in 0..3 {
                    cols[i].vertical_centered(|ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                            let avail_w = ui.available_width();
                            // Total width estimate: label(25) + space(4) + knob(36) + space(4) + stereo_vu(18) = 87
                            ui.add_space(((avail_w - 87.0) / 2.0).max(0.0));

                            ui.label(RichText::new(labels[i]).size(8.0).strong().color(Color32::from_gray(100)));

                            let (gain, peak_hold, target_id) = match i {
                                0 => (&mut app.booth_gain, &mut app.booth_peak_hold, 22),
                                1 => (&mut app.rec_gain, &mut app.rec_peak_hold, 23),
                                _ => (&mut app.master_gain, &mut app.master_peak_hold, 21),
                            };

                            if widgets::render_knob(ui, gain, 0.0..=1.5, "", colors[i]).changed() {
                                let _ = app.command_sender.send(nullherz_traits::Command::SetParam { target_id, param_id: 0, value: *gain, ramp_duration_samples: 128 });
                            }

                            let peak = if i == 2 {
                                telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2))
                            } else {
                                telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2)) * (*gain)
                            };
                            if peak > *peak_hold { *peak_hold = peak; } else { *peak_hold *= 0.98; }

                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                                widgets::render_vu_meter(ui, peak * 0.95, *peak_hold * 0.95, colors[i], 36.0);
                                widgets::render_vu_meter(ui, peak, *peak_hold, colors[i], 36.0);
                            });
                        });
                    });
                }
            });
            ui.add_space(8.0);

            // 2. CHANNEL STRIPS
            ui.horizontal_top(|ui| {
                let col_w = (inner_rect.width() - 12.0) / 4.0;
                ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

                for i in 0..4 {
                    ui.allocate_ui(vec2(col_w, ui.available_height() - 60.0), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(2.0);

                            // CHANNEL HEADER
                            Frame::none().fill(InspectorApp::deck_color(i).linear_multiply(0.2)).rounding(2.0).inner_margin(4.0).show(ui, |ui| {
                                ui.set_width(col_w - 4.0);
                                ui.label(RichText::new(format!("CH{}", i + 1)).small().strong().color(InspectorApp::deck_color(i)));
                            });
                            ui.add_space(4.0);

                            // GAIN / TRIM
                            if widgets::render_knob(ui, &mut app.channel_trims[i], 0.0..=2.0, "TRIM", InspectorApp::deck_color(i)).changed() {
                                let _ = app.command_sender.send(nullherz_traits::Command::SetParam {
                                    target_id: (i as u64 * 4 + 1),
                                    param_id: 0,
                                    value: app.channel_trims[i] * app.channel_faders[i],
                                    ramp_duration_samples: 128,
                                });
                            }
                            ui.add_space(4.0);

                            // HI / MID / LOW EQ
                            for (label, param_idx, state_val) in [("HI", 2, &mut app.channel_eq_high[i]), ("MID", 1, &mut app.channel_eq_mid[i]), ("LOW", 0, &mut app.channel_eq_low[i])] {
                                if widgets::render_knob(ui, state_val, 0.0..=2.0, label, InspectorApp::deck_color(i)).changed() {
                                    let _ = app.command_sender.send(nullherz_traits::Command::SetParam {
                                        target_id: (i as u64 * 4 + 3),
                                        param_id: param_idx,
                                        value: *state_val,
                                        ramp_duration_samples: 0,
                                    });
                                }
                                ui.add_space(4.0);
                            }

                            // FADER & VU (Bottom of strip)
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                let fader_h = 140.0;
                                let fader_w = 20.0;
                                let spacing = 4.0;
                                let left_pad = (col_w - fader_w - 8.0 - spacing) / 2.0;
                                ui.add_space(left_pad.max(0.0));

                                if widgets::render_fader(ui, &mut app.channel_faders[i], 0.0..=1.0, InspectorApp::deck_color(i), fader_h, 24.0).changed() {
                                    let _ = app.command_sender.send(nullherz_traits::Command::SetParam {
                                        target_id: (i as u64 * 4 + 1),
                                        param_id: 0,
                                        value: app.channel_trims[i] * app.channel_faders[i],
                                        ramp_duration_samples: 128,
                                    });
                                }

                                ui.add_space(spacing);
                                let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1].min(1.2));
                                if peak > app.channel_peak_hold[i] { app.channel_peak_hold[i] = peak; }
                                else { app.channel_peak_hold[i] *= 0.98; }
                                widgets::render_vu_meter(ui, peak, app.channel_peak_hold[i], Color32::from_rgb(0, 255, 180), fader_h);
                            });
                        });
                    });
                }
            });

            ui.add_space(10.0);

            // 3. BOTTOM: GLOBAL CROSSFADER
            Frame::none()
                .fill(Color32::from_rgb(10, 10, 12))
                .rounding(PANEL_ROUNDING)
                .stroke(Stroke::new(1.0, Color32::from_gray(30)))
                .inner_margin(INNER_MARGIN)
                .show(ui, |ui| {
                    ui.set_width(inner_rect.width() - 4.0);
                    ui.vertical_centered(|ui| {
                        ui.horizontal(|ui| {
                            let total_w = ui.available_width();
                            ui.add_space(total_w / 2.0 - 35.0);
                            ui.label(RichText::new("X-FADE").small().strong().color(Color32::from_gray(100)));
                            if ui.add(egui::Button::new(if app.crossfader_curve > 0.5 { "POW" } else { "LIN" }).small()).clicked() {
                                app.crossfader_curve = if app.crossfader_curve > 0.5 { 0.0 } else { 1.0 };
                                for target_id in [16, 17] {
                                    let _ = app.command_sender.send(nullherz_traits::Command::SetParam { target_id, param_id: 1, value: app.crossfader_curve, ramp_duration_samples: 0 });
                                }
                            }
                        });
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("A").color(Color32::from_rgb(0, 200, 255)));
                            let total_w = ui.available_width();
                            let x_res = widgets::render_horizontal_fader(ui, &mut app.crossfader_pos, 0.0..=1.0, Color32::WHITE, total_w - 25.0, 30.0);
                            if x_res.changed() {
                                for target_id in [16, 17] {
                                    let _ = app.command_sender.send(nullherz_traits::Command::SetParam { target_id, param_id: 0, value: app.crossfader_pos, ramp_duration_samples: 0 });
                                }
                            }
                            ui.label(RichText::new("B").color(Color32::from_rgb(0, 255, 150)));
                        });
                    });
                });
        });
    });
}
