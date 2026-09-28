use egui::{Ui, Frame, Margin, Rounding, Stroke, RichText, ScrollArea, Color32, Pos2, Vec2};
use crate::InspectorApp;
use crate::state::{ChannelInputSource, MasterOutput};
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;

/// Fixed strip width: every card is the same size regardless of window width.
const STRIP_W: f32 = 140.0;
const FADER_H: f32 = 150.0;
const VERTICAL_WAVEFORM_H: f32 = 225.0; // 25% extra height (180.0 * 1.25)

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    ui.heading(RichText::new("System Mixer").size(theme.type_heading));

    // Align channel list and master to the bottom of the mixer page so space is above the racks
    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.add_space(theme.space_md);

        ui.horizontal_top(|ui| {
            let scroll_width = (ui.available_width() - STRIP_W - theme.space_md).max(100.0);

            // Channel strips inside horizontal ScrollArea (only channels scroll)
            ScrollArea::horizontal()
                .id_source("sys_mixer_scroll")
                .max_width(scroll_width)
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        let num_ch = app.mixer.num_channels.clamp(1, 16);
                        for i in 0..num_ch {
                            render_channel_strip(app, ui, i, telemetry);
                            ui.add_space(theme.space_sm);
                        }

                        // Add "+" channel button if under max limit (16 channels)
                        if num_ch < 16 {
                            render_add_channel_button(app, ui);
                        }
                    });
                });

            // Master channel stays outside ScrollArea, always pinned to the far right
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                render_master_strip(app, ui, telemetry);
            });
        });
    });
}

fn render_add_channel_button(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .inner_margin(Margin::same(theme.space_md))
        .stroke(Stroke::new(1.0_f32, theme.border))
        .show(ui, |ui| {
            ui.set_width(STRIP_W);
            ui.vertical_centered(|ui| {
                ui.add_space(180.0);
                let btn = egui::Button::new(RichText::new("+").size(24.0).strong().color(theme.accent))
                    .fill(theme.bg_inset)
                    .min_size(Vec2::new(50.0, 50.0));
                if ui.add(btn).on_hover_text("Add Channel Strip").clicked() {
                    if app.mixer.num_channels < 16 {
                        app.mixer.num_channels += 1;
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new("ADD CHANNEL").size(9.0).strong().color(theme.text_secondary));
            });
        });
}

const NEEDLE_WINDOW_SECS: f32 = 8.0;

fn render_vertical_waveform(
    app: &mut InspectorApp,
    ui: &mut Ui,
    deck_idx: usize,
    elapsed_samples: u64,
    peak_level: f32,
    deck_color: Color32,
    theme: &nullherz_ui_hal::Theme,
    telemetry: &Option<Telemetry>,
) {
    let wf_w = ui.available_width();
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(wf_w, VERTICAL_WAVEFORM_H), egui::Sense::hover());

    // Background inset matching DJ Studio Console waveform canvas
    ui.painter().rect_filled(rect, theme.radius_sm, theme.bg_inset);
    ui.painter().rect_stroke(rect, theme.radius_sm, Stroke::new(1.0, theme.border_stroke.color));

    let style = app.mixer.waveform_styles.get(deck_idx).copied().unwrap_or(nullherz_ui_hal::render::waveform_renderer::WaveformStyle::MultiBand);
    let track = app.decks.cached_tracks.get(deck_idx).and_then(|t| t.clone());

    if let Some(t) = track {
        let sr = (t.metadata.sample_rate.max(1)) as f32;
        let total_frames = t.metadata.total_samples.max(1);

        let window_frames = NEEDLE_WINDOW_SECS * sr;
        let center = elapsed_samples as f32;
        let win_start = center as f64 - (window_frames as f64) * 0.5;
        let win_end = center as f64 + (window_frames as f64) * 0.5;
        let start_ratio = (win_start / total_frames as f64) as f32;
        let end_ratio = (win_end / total_frames as f64) as f32;

        let eq_gains = if deck_idx < 16 {
            [
                app.mixer.channel_eq_low[deck_idx],
                app.mixer.channel_eq_mid[deck_idx],
                app.mixer.channel_eq_high[deck_idx],
            ]
        } else {
            [1.0, 1.0, 1.0]
        };

        if let Some(wf_lock) = app.deck_waveform_renderers.get(deck_idx % 4).and_then(|opt| opt.as_ref()) {
            let mut wf = wf_lock.lock();
            let color = deck_color.to_array().map(|v| v as f32 / 255.0);

            if let Some(wgpu) = &app.wgpu_renderer {
                let wgpu = wgpu.lock();
                wf.update_globals(&wgpu.queue, 0.0, 1.0, true, style, color);
                if t.metadata.band_waveform.is_empty() {
                    wf.update_from_mip_window(&wgpu.queue, &t.metadata.mip_waveform, start_ratio, end_ratio, rect.height() as u32, color);
                } else {
                    wf.update_from_band_window(&wgpu.queue, &t.metadata.band_waveform, start_ratio, end_ratio, rect.height() as u32, style, color, eq_gains);
                }
            }

            nullherz_ui_hal::render::waveform_renderer::ui_paint_waveform(ui, rect, wf_lock.clone());
        } else {
            // Draw identical vertical painter fallback matching NEEDLE_WINDOW_SECS
            let center_x = rect.center().x;
            let height = rect.height();
            let slices = 50;
            let painter = ui.painter();

            let band_wf = &t.metadata.band_waveform;
            if !band_wf.is_empty() && !band_wf.low.levels.is_empty() {
                if let (Some(low_lvl), Some(mid_lvl), Some(high_lvl)) = (
                    band_wf.low.levels.get(0), band_wf.mid.levels.get(0), band_wf.high.levels.get(0),
                ) {
                    let num_windows = low_lvl.len();
                    for slice_i in 0..slices {
                        let slice_ratio = slice_i as f32 / slices as f32;
                        let frame_pos = win_end - (slice_ratio as f64) * (win_end - win_start);
                        let idx_ratio = (frame_pos / total_frames as f64).clamp(0.0, 1.0) as f32;
                        let idx = (idx_ratio * num_windows as f32) as usize;

                        let low_val = low_lvl.get(idx).copied().unwrap_or(0.05).abs().clamp(0.01, 1.0) * eq_gains[0];
                        let mid_val = mid_lvl.get(idx).copied().unwrap_or(0.05).abs().clamp(0.01, 1.0) * eq_gains[1];
                        let high_val = high_lvl.get(idx).copied().unwrap_or(0.05).abs().clamp(0.01, 1.0) * eq_gains[2];

                        let y = rect.min.y + slice_ratio * height;
                        let is_past = y > rect.center().y;
                        let alpha_mult = if is_past { 0.5 } else { 1.0 };

                        let col = match style {
                            nullherz_ui_hal::render::waveform_renderer::WaveformStyle::MultiBand => {
                                let r = (low_val * 255.0 * alpha_mult) as u8;
                                let g = (mid_val * 220.0 * alpha_mult) as u8;
                                let b = (high_val * 255.0 * alpha_mult) as u8;
                                Color32::from_rgb(r.max(30), g.max(30), b.max(60))
                            }
                            nullherz_ui_hal::render::waveform_renderer::WaveformStyle::Mono => {
                                deck_color.linear_multiply(if is_past { 0.5 } else { 1.0 })
                            }
                            nullherz_ui_hal::render::waveform_renderer::WaveformStyle::PhonLoudness => {
                                let pw = (mid_val * 1.3 + high_val * 1.1 + low_val * 0.5).clamp(0.0, 1.0);
                                Color32::from_rgb(
                                    (100.0 + pw * 155.0) as u8,
                                    (20.0 + pw * 220.0) as u8,
                                    ((1.0 - pw) * 180.0) as u8,
                                )
                            }
                            nullherz_ui_hal::render::waveform_renderer::WaveformStyle::SpectrumHeatmap => {
                                let amp = low_val * 0.5 + mid_val * 0.35 + high_val * 0.15;
                                Color32::from_rgb((amp * 255.0) as u8, ((1.0 - amp) * 200.0) as u8, 120)
                            }
                            nullherz_ui_hal::render::waveform_renderer::WaveformStyle::Outline => deck_color,
                        };

                        let total_amp = (low_val * 0.5 + mid_val * 0.35 + high_val * 0.15).clamp(0.02, 1.0);
                        let bar_w = total_amp * (rect.width() * 0.46);

                        painter.line_segment(
                            [Pos2::new(center_x - bar_w, y), Pos2::new(center_x + bar_w, y)],
                            Stroke::new(2.0, col),
                        );
                    }
                }
            } else {
                let peaks = &t.metadata.peaks;
                if !peaks.is_empty() {
                    let num_peaks = peaks.len();
                    for slice_i in 0..slices {
                        let slice_ratio = slice_i as f32 / slices as f32;
                        let frame_pos = win_end - (slice_ratio as f64) * (win_end - win_start);
                        let idx_ratio = (frame_pos / total_frames as f64).clamp(0.0, 1.0) as f32;
                        let peak_i = (idx_ratio * num_peaks as f32) as usize;

                        let amp = peaks.get(peak_i).copied().unwrap_or(0.05).abs().clamp(0.02, 1.0);
                        let bar_w = amp * (rect.width() * 0.45);
                        let y = rect.min.y + slice_ratio * height;

                        painter.line_segment(
                            [Pos2::new(center_x - bar_w, y), Pos2::new(center_x + bar_w, y)],
                            Stroke::new(2.0, deck_color.linear_multiply(0.8)),
                        );
                    }
                }
            }
        }

        // Beat grid
        let to_y = |frame_pos: f64| -> f32 {
            rect.min.y + ((win_end - frame_pos) / (win_end - win_start)) as f32 * rect.height()
        };

        let center_x = rect.center().x;
        let painter = ui.painter();
        if t.metadata.bpm > 20.0 {
            let spb = sr as f64 * 60.0 / t.metadata.bpm as f64;
            let offset = t.metadata.beat_grid_offset as f64;
            let first_beat = (((win_start - offset) / spb).floor().max(0.0)) as u64;
            let mut b = first_beat;
            loop {
                let bpos = offset + b as f64 * spb;
                if bpos > win_end || bpos > total_frames as f64 || b > first_beat + 512 {
                    break;
                }
                if bpos >= win_start {
                    let y = to_y(bpos);
                    let (w, alpha) = if b % 4 == 0 {
                        (rect.width() * 0.9, 65)
                    } else {
                        (rect.width() * 0.4, 35)
                    };
                    painter.line_segment(
                        [Pos2::new(center_x - w * 0.5, y), Pos2::new(center_x + w * 0.5, y)],
                        Stroke::new(1.0, Color32::from_white_alpha(alpha)),
                    );

                    let beat_num = (b % 4) + 1;
                    painter.text(
                        Pos2::new(center_x + w * 0.5 + 2.0, y - 4.0),
                        egui::Align2::LEFT_CENTER,
                        format!("{}", beat_num),
                        egui::FontId::monospace(8.0),
                        if b % 4 == 0 { theme.accent } else { theme.text_disabled },
                    );
                }
                b += 1;
            }
        }

        // Hot cues
        for (ci, cue) in t.metadata.hot_cues.iter().enumerate() {
            if let Some(p) = cue {
                let p = *p as f64;
                if p >= win_start && p <= win_end {
                    let y = to_y(p);
                    painter.line_segment(
                        [Pos2::new(rect.min.x, y), Pos2::new(rect.min.x + 10.0, y)],
                        Stroke::new(2.0, theme.accent),
                    );
                    painter.text(
                        Pos2::new(rect.min.x + 12.0, y - 4.0),
                        egui::Align2::LEFT_CENTER,
                        format!("{}", ci + 1),
                        egui::FontId::monospace(9.0),
                        theme.accent,
                    );
                }
            }
        }

        // Center playhead needle
        let playhead_y = rect.center().y;
        let is_playing = app.decks.deck_playing.get(deck_idx % 4).copied().unwrap_or(false);
        if is_playing {
            let beat_pos = telemetry.as_ref().map(|tel| tel.beat_position as f32).unwrap_or(0.0);
            let pulse = (1.0 - (beat_pos % 1.0)).powf(3.0);
            let glow_color = deck_color.linear_multiply(pulse * 0.8);
            painter.line_segment(
                [Pos2::new(rect.min.x + 1.0, playhead_y), Pos2::new(rect.max.x - 1.0, playhead_y)],
                Stroke::new(5.0 + pulse * 6.0, glow_color),
            );
        }

        painter.line_segment(
            [Pos2::new(rect.min.x + 1.0, playhead_y), Pos2::new(rect.max.x - 1.0, playhead_y)],
            Stroke::new(3.0, Color32::from_black_alpha(160)),
        );
        painter.line_segment(
            [Pos2::new(rect.min.x + 1.0, playhead_y), Pos2::new(rect.max.x - 1.0, playhead_y)],
            Stroke::new(1.5, theme.text_primary),
        );

        return;
    }

    // Fallback: Real-time dynamic signal visualizer when no track loaded
    let slices = 25;
    let center_x = rect.center().x;
    let height = rect.height();
    for slice_i in 0..slices {
        let y = rect.min.y + (slice_i as f32 / slices as f32) * height;
        let factor = (slice_i as f32 * 0.3 + peak_level * 5.0).sin().abs();
        let amp = (peak_level * factor).clamp(0.03, 0.95);
        let bar_w = amp * (rect.width() * 0.45);

        ui.painter().line_segment(
            [Pos2::new(center_x - bar_w, y), Pos2::new(center_x + bar_w, y)],
            Stroke::new(2.0, deck_color.linear_multiply(0.6)),
        );
    }
}

fn render_channel_strip(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let deck_color = crate::InspectorApp::deck_color(&theme, i % 4);
    let deck_char_letter = (b'a' + (i % 26) as u8) as char;

    // Resolve this channel's REAL node ids from the telemetry node map.
    let gain_node = app.topo.node_map.get(&format!("deck_{}_gain", deck_char_letter)).copied();
    let iso_node = app.topo.node_map.get(&format!("deck_{}_isolator", deck_char_letter)).copied();
    let meter_node = iso_node.or_else(|| app.topo.node_map.get(&format!("deck_{}_sampler", deck_char_letter)).copied());

    let level_base = if let (Some(t), Some(node)) = (telemetry, meter_node) {
        t.peak_levels.get(node as usize).copied().unwrap_or(0.0)
    } else {
        0.0
    };

    let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions.get(i).copied().unwrap_or(0)).unwrap_or(0);
    let is_focused = app.decks.focused_deck == i;

    let border_stroke = if is_focused {
        Stroke::new(2.0, deck_color)
    } else {
        Stroke::new(1.0_f32, theme.border)
    };

    let fill_color = if is_focused {
        theme.bg_surface.linear_multiply(1.2)
    } else {
        theme.bg_surface
    };

    Frame::none()
        .fill(fill_color)
        .rounding(Rounding::same(theme.radius_md))
        .inner_margin(Margin::same(theme.space_md))
        .stroke(border_stroke)
        .show(ui, |ui| {
            ui.set_width(STRIP_W);
            ui.vertical(|ui| {
                let header_resp = ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 40.0).max(0.0) / 2.0);
                    ui.label(RichText::new(format!("CH {}", (b'A' + (i % 26) as u8) as char)).strong().size(theme.type_body).color(deck_color));
                });
                if header_resp.response.interact(egui::Sense::click()).clicked() {
                    app.decks.focused_deck = i;
                }
                ui.add_space(theme.space_xs);

                // Waveform Style Dropdown Selector
                ui.horizontal(|ui| {
                    let avail_w = ui.available_width();
                    let selected_style = app.mixer.waveform_styles[i];
                    egui::ComboBox::from_id_source(format!("ch_wf_style_{}", i))
                        .selected_text(RichText::new(selected_style.name()).size(8.5).strong().color(theme.text_primary))
                        .width(avail_w)
                        .show_ui(ui, |ui| {
                            for st in nullherz_ui_hal::render::waveform_renderer::WaveformStyle::all() {
                                ui.selectable_value(&mut app.mixer.waveform_styles[i], *st, st.name());
                            }
                        });
                });
                ui.add_space(2.0);

                // Vertical Waveform Canvas
                render_vertical_waveform(app, ui, i, elapsed_samples, level_base, deck_color, &theme, telemetry);
                ui.add_space(theme.space_xs);

                // --- INSERTS RACK ---
                ui.group(|ui| {
                    ui.set_width(STRIP_W - 12.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("CHANNEL STRIP").size(theme.type_caption).strong().color(theme.text_secondary));
                        ui.add_space(2.0);

                        // 1. TRIM / GAIN Knob
                        Frame::none()
                            .fill(theme.bg_inset)
                            .rounding(Rounding::same(theme.radius_sm))
                            .inner_margin(Margin::same(4.0))
                            .stroke(Stroke::new(1.0, theme.border_stroke.color))
                            .show(ui, |ui| {
                                ui.set_width(STRIP_W - 20.0);
                                ui.vertical_centered(|ui| {
                                    let mut gain_val = app.mixer.channel_gain[i];
                                    if widgets::render_knob_sized(ui, &mut gain_val, 0.0..=2.0, "GAIN", deck_color, 28.0).changed() {
                                        app.mixer.channel_gain[i] = gain_val;
                                        if let Some(gain_id) = gain_node {
                                            let net_gain = gain_val * app.mixer.channel_faders[i];
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: gain_id as u64,
                                                param_id: 0,
                                                value: net_gain,
                                                ramp_duration_samples: 128,
                                            }));
                                        }
                                    }
                                });
                            });

                        ui.add_space(4.0);

                        // 2. 3-BAND EQ (HIGH, MID, LOW vertically aligned)
                        Frame::none()
                            .fill(theme.bg_inset)
                            .rounding(Rounding::same(theme.radius_sm))
                            .inner_margin(Margin::same(4.0))
                            .stroke(Stroke::new(1.0, theme.border_stroke.color))
                            .show(ui, |ui| {
                                ui.set_width(STRIP_W - 20.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new("3-BAND EQ").size(9.0).strong().color(theme.accent));
                                    ui.add_space(2.0);

                                    // HIGH Knob
                                    let mut hi = app.mixer.channel_eq_high[i];
                                    if widgets::render_knob_sized(ui, &mut hi, 0.0..=2.0, "HIGH", deck_color, 26.0).changed() {
                                        app.mixer.channel_eq_high[i] = hi;
                                        if let Some(node_id) = iso_node {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: node_id as u64,
                                                param_id: 2, // HI param
                                                value: hi,
                                                ramp_duration_samples: 128,
                                            }));
                                        }
                                    }

                                    ui.add_space(2.0);

                                    // MID Knob
                                    let mut mid = app.mixer.channel_eq_mid[i];
                                    if widgets::render_knob_sized(ui, &mut mid, 0.0..=2.0, "MID", deck_color, 26.0).changed() {
                                        app.mixer.channel_eq_mid[i] = mid;
                                        if let Some(node_id) = iso_node {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: node_id as u64,
                                                param_id: 1, // MID param
                                                value: mid,
                                                ramp_duration_samples: 128,
                                            }));
                                        }
                                    }

                                    ui.add_space(2.0);

                                    // LOW Knob
                                    let mut low = app.mixer.channel_eq_low[i];
                                    if widgets::render_knob_sized(ui, &mut low, 0.0..=2.0, "LOW", deck_color, 26.0).changed() {
                                        app.mixer.channel_eq_low[i] = low;
                                        if let Some(node_id) = iso_node {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: node_id as u64,
                                                param_id: 0, // LOW param
                                                value: low,
                                                ramp_duration_samples: 128,
                                            }));
                                        }
                                    }
                                });
                            });

                        ui.add_space(4.0);

                        // 3. PITCH / SPEED
                        Frame::none()
                            .fill(theme.bg_inset)
                            .rounding(Rounding::same(theme.radius_sm))
                            .inner_margin(Margin::same(4.0))
                            .stroke(Stroke::new(1.0, theme.border_stroke.color))
                            .show(ui, |ui| {
                                ui.set_width(STRIP_W - 20.0);
                                ui.vertical_centered(|ui| {
                                    let mut pitch_val = app.mixer.channel_pitch[i];
                                    if widgets::render_knob_sized(ui, &mut pitch_val, 0.5..=1.5, "PITCH", deck_color, 26.0).changed() {
                                        app.mixer.channel_pitch[i] = pitch_val;
                                        let deck_char = (b'A' + (i % 26) as u8) as char;
                                        let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetDeckParam {
                                            deck_id: deck_char,
                                            param_type: nullherz_traits::DeckParamType::Pitch,
                                            value: pitch_val,
                                        }));
                                    }
                                });
                            });

                        ui.add_space(4.0);

                        // 4. ADDED FX INSERT SLOT
                        let insert_node = app.topo.node_map.get(&format!("deck_{}_insert", deck_char_letter))
                            .or_else(|| app.topo.node_map.get(&format!("deck_{}_fx1", deck_char_letter)))
                            .copied();

                        let mut remove_insert = false;
                        if let Some(ref name) = app.decks.deck_inserts[i] {
                            let name_clone = name.clone();
                            let is_mutator = name_clone.to_lowercase().contains("mutator");

                            Frame::none()
                                .fill(theme.accent.linear_multiply(0.15))
                                .rounding(Rounding::same(theme.radius_sm))
                                .inner_margin(Margin::same(4.0))
                                .stroke(Stroke::new(1.0, theme.accent))
                                .show(ui, |ui| {
                                    ui.set_width(STRIP_W - 20.0);
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(format!("FX: {}", name_clone)).size(9.0).strong().color(theme.text_primary));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                let close_btn = egui::Button::new(RichText::new("×").size(11.0).strong().color(theme.text_secondary))
                                                    .fill(Color32::TRANSPARENT)
                                                    .min_size(Vec2::new(14.0, 14.0));
                                                if ui.add(close_btn).on_hover_text("Remove FX").clicked() {
                                                    remove_insert = true;
                                                }
                                            });
                                        });

                                        ui.add_space(2.0);

                                        if is_mutator {
                                            // MUTATOR 6 Parallel Mutation Macro Knobs
                                            ui.vertical_centered(|ui| {
                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 2.0;

                                                    // FLESH (Param 0)
                                                    let mut flesh_val = app.decks.deck_insert_params[i][0];
                                                    if widgets::render_knob_sized(ui, &mut flesh_val, 0.0..=1.0, "FLESH", deck_color, 22.0).changed() {
                                                        app.decks.deck_insert_params[i][0] = flesh_val;
                                                        if let Some(target_id) = insert_node {
                                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                                target_id: target_id as u64,
                                                                param_id: 0,
                                                                value: flesh_val,
                                                                ramp_duration_samples: 128,
                                                            }));
                                                        }
                                                    }

                                                    // BONE (Param 1)
                                                    let mut bone_val = app.decks.deck_insert_params[i][1];
                                                    if widgets::render_knob_sized(ui, &mut bone_val, 0.0..=1.0, "BONE", deck_color, 22.0).changed() {
                                                        app.decks.deck_insert_params[i][1] = bone_val;
                                                        if let Some(target_id) = insert_node {
                                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                                target_id: target_id as u64,
                                                                param_id: 1,
                                                                value: bone_val,
                                                                ramp_duration_samples: 128,
                                                            }));
                                                        }
                                                    }

                                                    // TEETH (Param 2)
                                                    let mut teeth_val = app.decks.deck_insert_params[i][2];
                                                    if widgets::render_knob_sized(ui, &mut teeth_val, 0.0..=1.0, "TEETH", deck_color, 22.0).changed() {
                                                        app.decks.deck_insert_params[i][2] = teeth_val;
                                                        if let Some(target_id) = insert_node {
                                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                                target_id: target_id as u64,
                                                                param_id: 2,
                                                                value: teeth_val,
                                                                ramp_duration_samples: 128,
                                                            }));
                                                        }
                                                    }
                                                });

                                                ui.add_space(2.0);

                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 2.0;

                                                    // PARASITE (Param 3)
                                                    let mut para_val = app.decks.deck_insert_params[i][3];
                                                    if widgets::render_knob_sized(ui, &mut para_val, 0.0..=1.0, "PARA", deck_color, 22.0).changed() {
                                                        app.decks.deck_insert_params[i][3] = para_val;
                                                        if let Some(target_id) = insert_node {
                                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                                target_id: target_id as u64,
                                                                param_id: 3,
                                                                value: para_val,
                                                                ramp_duration_samples: 128,
                                                            }));
                                                        }
                                                    }

                                                    // ASYM (Param 4)
                                                    let mut asym_val = app.decks.deck_insert_params[i][4];
                                                    if widgets::render_knob_sized(ui, &mut asym_val, 0.0..=1.0, "ASYM", deck_color, 22.0).changed() {
                                                        app.decks.deck_insert_params[i][4] = asym_val;
                                                        if let Some(target_id) = insert_node {
                                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                                target_id: target_id as u64,
                                                                param_id: 4,
                                                                value: asym_val,
                                                                ramp_duration_samples: 128,
                                                            }));
                                                        }
                                                    }

                                                    // ABOM (Param 5)
                                                    let mut abom_val = app.decks.deck_insert_params[i][5];
                                                    if widgets::render_knob_sized(ui, &mut abom_val, 0.0..=1.0, "ABOM", deck_color, 22.0).changed() {
                                                        app.decks.deck_insert_params[i][5] = abom_val;
                                                        if let Some(target_id) = insert_node {
                                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                                target_id: target_id as u64,
                                                                param_id: 5,
                                                                value: abom_val,
                                                                ramp_duration_samples: 128,
                                                            }));
                                                        }
                                                    }
                                                });
                                            });
                                        } else {
                                            // Generic Insert Controls (Drive / Mix)
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 4.0;
                                                let mut drive_val = app.decks.deck_insert_params[i][0];
                                                if widgets::render_knob_sized(ui, &mut drive_val, 0.0..=5.0, "DRIVE", deck_color, 22.0).changed() {
                                                    app.decks.deck_insert_params[i][0] = drive_val;
                                                    if let Some(target_id) = insert_node {
                                                        let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                            target_id: target_id as u64,
                                                            param_id: 0,
                                                            value: drive_val,
                                                            ramp_duration_samples: 128,
                                                        }));
                                                    }
                                                }

                                                let mut mix_val = app.decks.deck_insert_params[i][7];
                                                if widgets::render_knob_sized(ui, &mut mix_val, 0.0..=1.0, "MIX", deck_color, 22.0).changed() {
                                                    app.decks.deck_insert_params[i][7] = mix_val;
                                                    if let Some(target_id) = insert_node {
                                                        let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                            target_id: target_id as u64,
                                                            param_id: 7,
                                                            value: mix_val,
                                                            ramp_duration_samples: 128,
                                                        }));
                                                    }
                                                }
                                            });
                                        }
                                    });
                                });
                        } else if ui.add_sized([STRIP_W - 20.0, 18.0], egui::Button::new(RichText::new("+ FX").size(9.0).strong()).fill(theme.bg_inset)).clicked() {
                            app.active_right_tab = Some(crate::RightTab::Store);
                            app.store.active_tag_filter = Some("insert".to_string());
                        }

                        if remove_insert {
                            app.decks.deck_inserts[i] = None;
                            if let Some(target_id) = insert_node {
                                let _ = app.command_sender.send(nullherz_traits::Command::Topology(
                                    nullherz_traits::TopologyCommand::SwapProcessor {
                                        node_idx: target_id,
                                        processor_type_id: nullherz_traits::ProcessorTypeId::BYPASS,
                                    }
                                ));
                            }
                        }
                    });
                });

                ui.add_space(theme.space_md);

                // --- VOLUME FADER & STEREO VU METERS ---
                ui.horizontal(|ui| {
                    let fader_w = 24.0;
                    let pad = (ui.available_width() - fader_w).max(0.0) / 2.0;
                    ui.add_space(pad);

                    let r_fader = widgets::render_fader(ui, &mut app.mixer.channel_faders[i], 0.0..=1.2, deck_color, FADER_H, 30.0);
                    if r_fader.changed()
                        && let Some(gain_id) = gain_node {
                            let net_gain = app.mixer.channel_gain[i] * app.mixer.channel_faders[i];
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: gain_id as u64,
                                param_id: 0,
                                value: net_gain,
                                ramp_duration_samples: 128,
                            }));
                        }
                    if r_fader.drag_stopped() || r_fader.lost_focus() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CheckpointParameterEdit));
                    }

                    ui.add_space(4.0);

                    // STEREO VU METERS (Left & Right channels side-by-side next to the centered fader)
                    if let (Some(t), Some(node)) = (telemetry, meter_node) {
                        let level_base = t.peak_levels.get(node as usize).copied().unwrap_or(0.0);
                        let bal = app.mixer.channel_balance[i];
                        let level_l = level_base * (1.0 - (bal - 0.5).max(0.0));
                        let level_r = level_base * (1.0 - (0.5 - bal).max(0.0));

                        widgets::render_vu_meter(ui, level_l, app.mixer.channel_peak_hold[i], deck_color, FADER_H);
                        ui.add_space(2.0);
                        widgets::render_vu_meter(ui, level_r, app.mixer.channel_peak_hold[i], deck_color, FADER_H);
                    } else {
                        widgets::render_vu_meter(ui, 0.0, 0.0, theme.text_disabled, FADER_H);
                        ui.add_space(2.0);
                        widgets::render_vu_meter(ui, 0.0, 0.0, theme.text_disabled, FADER_H);
                    }
                });

                ui.add_space(theme.space_xs);
                ui.vertical_centered(|ui| {
                    let net_gain = app.mixer.channel_gain[i] * app.mixer.channel_faders[i];
                    ui.label(
                        RichText::new(format!("{:+.1} dB", 20.0 * net_gain.max(1e-3).log10()))
                            .monospace()
                            .size(theme.type_caption)
                            .color(theme.text_secondary),
                    );
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // --- COMPACT DECK CONTROLS & TRACK INFO ---
                ui.allocate_ui_with_layout(
                    egui::vec2(STRIP_W - 2.0 * theme.space_md, 34.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions.get(i).copied().unwrap_or(0)).unwrap_or(0);
                        if let Some(ref track) = app.decks.cached_tracks[i] {
                            let sample_rate = track.metadata.sample_rate.max(1) as f64;
                            let total_secs = track.metadata.total_samples as f64 / sample_rate;
                            let elapsed_secs = elapsed_samples as f64 / sample_rate;
                            let remain_secs = (total_secs - elapsed_secs).max(0.0);
                            let rem_m = (remain_secs / 60.0) as u32;
                            let rem_s = (remain_secs % 60.0) as u32;

                            ui.label(RichText::new(&track.title).size(10.0).strong().color(theme.text_primary));
                            if !track.artist.is_empty() {
                                ui.label(RichText::new(&track.artist).size(9.0).color(theme.text_secondary));
                            } else {
                                ui.label(RichText::new("—").size(9.0).color(theme.text_disabled));
                            }
                            let effective_bpm = track.metadata.bpm * app.mixer.channel_pitch[i];
                            ui.horizontal(|ui| {
                                ui.add_space((STRIP_W - 2.0 * theme.space_md - 110.0).max(0.0) / 2.0);
                                ui.label(RichText::new(format!("{:.1} BPM", effective_bpm)).monospace().size(9.0).strong().color(deck_color));
                                ui.label(RichText::new(format!("-{:02}:{:02}", rem_m, rem_s)).monospace().size(9.0).color(theme.text_secondary));
                            });
                        } else {
                            ui.label(RichText::new("No Track Loaded").size(9.0).italics().color(theme.text_disabled));
                            ui.label(RichText::new("—").size(9.0).color(theme.text_disabled));
                            ui.label(RichText::new("--.- BPM  --:--").monospace().size(9.0).color(theme.text_disabled));
                        }
                    },
                );

                ui.add_space(4.0);

                // Input Source Dropdown above transport buttons
                ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    let selected_source = app.mixer.channel_input_sources[i];
                    egui::ComboBox::from_id_source(format!("ch_input_src_{}", i))
                        .selected_text(RichText::new(selected_source.name()).size(9.0).strong().color(theme.text_primary))
                        .width(STRIP_W - 20.0)
                        .show_ui(ui, |ui| {
                            for src in ChannelInputSource::all() {
                                ui.selectable_value(&mut app.mixer.channel_input_sources[i], *src, src.name());
                            }
                        });
                });
                ui.add_space(4.0);

                // Transport Row: Single Play/Stop Toggle + CUE Button
                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 2.0 * theme.space_md - 96.0).max(0.0) / 2.0);

                    let is_playing = app.decks.deck_playing[i];
                    let play_icon = if is_playing { egui_phosphor::regular::PAUSE } else { egui_phosphor::regular::PLAY };
                    let play_btn = if is_playing {
                        egui::Button::new(RichText::new(play_icon).size(12.0).strong()).fill(theme.accent)
                    } else {
                        egui::Button::new(RichText::new(play_icon).size(12.0).strong()).fill(theme.bg_inset)
                    };

                    let deck_char_upper = (b'A' + (i % 26) as u8) as char;
                    if ui.add_sized([45.0, 22.0], play_btn).clicked() {
                        app.decks.focused_deck = i;
                        app.decks.deck_playing[i] = !is_playing;
                        if app.decks.deck_playing[i] {
                            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::PlayDeck { deck_id: deck_char_upper }));
                        } else {
                            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::StopDeck { deck_id: deck_char_upper }));
                        }
                    }

                    if ui.add_sized([45.0, 22.0], egui::Button::new(RichText::new("CUE").size(10.0).strong()).fill(theme.bg_inset)).clicked() {
                        app.decks.focused_deck = i;
                        let node_name = format!("deck_{}_sampler", (b'a' + (i % 26) as u8) as char);
                        if let Some(node_idx) = app.get_node_id(&node_name) {
                            let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpToHotCue { node_idx, cue_idx: 0 }));
                        }
                    }
                });
            });
        });
}

fn render_master_strip(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let accent = theme.accent;

    // Master level is applied per side: the SUMMING nodes' gain (param 0).
    let sum_l = app.topo.node_map.get("master_sum_l").copied();
    let sum_r = app.topo.node_map.get("master_sum_r").copied();

    let master_peak = app.viz.damped_master_peaks[0].max(app.viz.damped_master_peaks[1]);
    let master_deck_idx = app.decks.master_deck.unwrap_or(app.decks.focused_deck);
    let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions.get(master_deck_idx).copied().unwrap_or(0)).unwrap_or(0);

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .inner_margin(Margin::same(theme.space_md))
        .stroke(Stroke::new(1.0_f32, accent.gamma_multiply(0.6)))
        .show(ui, |ui| {
            ui.set_width(STRIP_W);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 52.0).max(0.0) / 2.0);
                    ui.label(RichText::new("MASTER").strong().size(theme.type_body).color(accent));
                });
                ui.add_space(theme.space_xs);

                // Waveform Style Dropdown Selector
                ui.horizontal(|ui| {
                    let avail_w = ui.available_width();
                    let selected_style = app.mixer.waveform_styles[master_deck_idx];
                    egui::ComboBox::from_id_source("master_wf_style")
                        .selected_text(RichText::new(selected_style.name()).size(8.5).strong().color(theme.text_primary))
                        .width(avail_w)
                        .show_ui(ui, |ui| {
                            for st in nullherz_ui_hal::render::waveform_renderer::WaveformStyle::all() {
                                ui.selectable_value(&mut app.mixer.waveform_styles[master_deck_idx], *st, st.name());
                            }
                        });
                });
                ui.add_space(2.0);

                // Master Vertical Signal Visualizer matching active master track waveform
                render_vertical_waveform(app, ui, master_deck_idx, elapsed_samples, master_peak, accent, &theme, telemetry);
                ui.add_space(theme.space_xs);

                // --- MASTER INSERTS RACK ---
                ui.group(|ui| {
                    ui.set_width(STRIP_W - 12.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("CHANNEL STRIP").size(theme.type_caption).strong().color(theme.text_secondary));
                        ui.add_space(2.0);

                        // 1. TRIM / MASTER GAIN Knob
                        Frame::none()
                            .fill(theme.bg_inset)
                            .rounding(Rounding::same(theme.radius_sm))
                            .inner_margin(Margin::same(4.0))
                            .stroke(Stroke::new(1.0, theme.border_stroke.color))
                            .show(ui, |ui| {
                                ui.set_width(STRIP_W - 20.0);
                                ui.vertical_centered(|ui| {
                                    let mut m_gain = app.mixer.master_gain;
                                    if widgets::render_knob_sized(ui, &mut m_gain, 0.0..=2.0, "GAIN", accent, 28.0).changed() {
                                        app.mixer.master_gain = m_gain;
                                        for node in [sum_l, sum_r].into_iter().flatten() {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: node as u64,
                                                param_id: 0,
                                                value: m_gain,
                                                ramp_duration_samples: 128,
                                            }));
                                        }
                                    }
                                });
                            });

                        ui.add_space(4.0);

                        // 2. 3-BAND EQ (HIGH, MID, LOW vertically aligned matching channel strips)
                        let master_eq_node = app.topo.node_map.get("master_eq").copied();
                        Frame::none()
                            .fill(theme.bg_inset)
                            .rounding(Rounding::same(theme.radius_sm))
                            .inner_margin(Margin::same(4.0))
                            .stroke(Stroke::new(1.0, theme.border_stroke.color))
                            .show(ui, |ui| {
                                ui.set_width(STRIP_W - 20.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new("3-BAND EQ").size(9.0).strong().color(accent));
                                    ui.add_space(2.0);

                                    // HIGH Knob
                                    let mut hi = app.mixer.mastering_eq_high;
                                    if widgets::render_knob_sized(ui, &mut hi, 0.0..=2.0, "HIGH", accent, 26.0).changed() {
                                        app.mixer.mastering_eq_high = hi;
                                        if let Some(node_id) = master_eq_node {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: node_id as u64,
                                                param_id: 2,
                                                value: hi,
                                                ramp_duration_samples: 128,
                                            }));
                                        }
                                    }

                                    ui.add_space(2.0);

                                    // MID Knob
                                    let mut mid = app.mixer.mastering_eq_mid;
                                    if widgets::render_knob_sized(ui, &mut mid, 0.0..=2.0, "MID", accent, 26.0).changed() {
                                        app.mixer.mastering_eq_mid = mid;
                                        if let Some(node_id) = master_eq_node {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: node_id as u64,
                                                param_id: 1,
                                                value: mid,
                                                ramp_duration_samples: 128,
                                            }));
                                        }
                                    }

                                    ui.add_space(2.0);

                                    // LOW Knob
                                    let mut low = app.mixer.mastering_eq_low;
                                    if widgets::render_knob_sized(ui, &mut low, 0.0..=2.0, "LOW", accent, 26.0).changed() {
                                        app.mixer.mastering_eq_low = low;
                                        if let Some(node_id) = master_eq_node {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: node_id as u64,
                                                param_id: 0,
                                                value: low,
                                                ramp_duration_samples: 128,
                                            }));
                                        }
                                    }
                                });
                            });

                        ui.add_space(4.0);

                        // 3. LIMITER / DYNAMICS
                        let limiter_node = app.topo.node_map.get("master_limiter").copied();
                        Frame::none()
                            .fill(theme.bg_inset)
                            .rounding(Rounding::same(theme.radius_sm))
                            .inner_margin(Margin::same(4.0))
                            .stroke(Stroke::new(1.0, theme.border_stroke.color))
                            .show(ui, |ui| {
                                ui.set_width(STRIP_W - 20.0);
                                ui.vertical_centered(|ui| {
                                    let mut thresh = 1.0f32;
                                    if widgets::render_knob_sized(ui, &mut thresh, 0.1..=1.0, "LIMIT", accent, 26.0).changed() {
                                        if let Some(lim_id) = limiter_node {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: lim_id as u64,
                                                param_id: 0,
                                                value: thresh,
                                                ramp_duration_samples: 128,
                                            }));
                                        }
                                    }
                                });
                            });

                        ui.add_space(4.0);

                        if ui.add_sized([STRIP_W - 20.0, 18.0], egui::Button::new(RichText::new("+ FX").size(9.0).strong()).fill(theme.bg_inset)).clicked() {
                            app.active_right_tab = Some(crate::RightTab::Store);
                            app.store.active_tag_filter = Some("insert".to_string());
                        }
                    });
                });

                ui.add_space(theme.space_md);

                // --- VOLUME FADER & STEREO VU METERS ---
                ui.horizontal(|ui| {
                    let fader_w = 24.0;
                    let pad = (ui.available_width() - fader_w).max(0.0) / 2.0;
                    ui.add_space(pad);

                    let r_fader = widgets::render_fader(ui, &mut app.mixer.master_gain, 0.0..=1.2, accent, FADER_H, 30.0);
                    if r_fader.changed() {
                        for node in [sum_l, sum_r].into_iter().flatten() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: node as u64,
                                param_id: 0,
                                value: app.mixer.master_gain,
                                ramp_duration_samples: 128,
                            }));
                        }
                    }
                    if r_fader.drag_stopped() || r_fader.lost_focus() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CheckpointParameterEdit));
                    }

                    ui.add_space(4.0);

                    // Stereo pair: damped master peaks are bound to master_sum_l/r in the update loop.
                    let _ = telemetry;
                    widgets::render_vu_meter(ui, app.viz.damped_master_peaks[0], app.mixer.master_peak_hold, accent, FADER_H);
                    ui.add_space(2.0);
                    widgets::render_vu_meter(ui, app.viz.damped_master_peaks[1], app.mixer.master_peak_hold, accent, FADER_H);
                });

                ui.add_space(theme.space_xs);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(format!("{:+.1} dB", 20.0 * app.mixer.master_gain.max(1e-3).log10()))
                            .monospace()
                            .size(theme.type_caption)
                            .color(theme.text_secondary),
                    );
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // Empty Track Info Area to keep master strip height & controls aligned with channels
                ui.allocate_ui_with_layout(
                    egui::vec2(STRIP_W - 2.0 * theme.space_md, 34.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        // Empty space matching exactly 3 lines (title, artist/placeholder, bpm/remaining)
                        ui.label(RichText::new("").size(10.0));
                        ui.label(RichText::new("").size(9.0));
                        ui.label(RichText::new("").size(9.0));
                    },
                );

                ui.add_space(4.0);

                // Master Output Selector Dropdown above transport button
                ui.horizontal(|ui| {
                    let avail_w = ui.available_width();
                    let selected_output = app.mixer.master_output_source;
                    egui::ComboBox::from_id_source("master_output_src")
                        .selected_text(RichText::new(selected_output.name()).size(9.0).strong().color(theme.text_primary))
                        .width(avail_w)
                        .show_ui(ui, |ui| {
                            for out in MasterOutput::all() {
                                ui.selectable_value(&mut app.mixer.master_output_source, *out, out.name());
                            }
                        });
                });
                ui.add_space(4.0);

                // Global Play/Stop Transport Toggle
                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 2.0 * theme.space_md - 96.0).max(0.0) / 2.0);

                    let is_playing = app.decks.global_playing;
                    let play_icon = if is_playing { egui_phosphor::regular::PAUSE } else { egui_phosphor::regular::PLAY };
                    let play_btn = if is_playing {
                        egui::Button::new(RichText::new(play_icon).size(12.0).strong()).fill(accent)
                    } else {
                        egui::Button::new(RichText::new(play_icon).size(12.0).strong()).fill(theme.bg_inset)
                    };

                    if ui.add_sized([96.0, 22.0], play_btn).clicked() {
                        app.decks.global_playing = !is_playing;
                        if app.decks.global_playing {
                            let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play));
                        } else {
                            let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop));
                        }
                    }
                });
            });
        });
}
