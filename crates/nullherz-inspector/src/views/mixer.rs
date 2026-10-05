use egui::{Ui, Frame, Margin, Rounding, Stroke, RichText, ScrollArea, Color32, Pos2, Vec2};
use crate::InspectorApp;
use crate::state::{ChannelInputSource, MasterOutput};
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;

/// Fixed strip width: every card is the same size regardless of window width.
pub const STRIP_W: f32 = 78.0;
#[allow(dead_code)]
pub const PAD_STRIP_W: f32 = 78.0;
pub const FADER_H: f32 = 110.0;
pub const VERTICAL_WAVEFORM_H: f32 = 200.0;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    ui.heading(RichText::new("System Mixer").size(theme.type_heading));

    // Align channel list and master to the bottom of the mixer page
    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.add_space(theme.space_md);

        ui.horizontal_top(|ui| {
            let scroll_width = (ui.available_width() - STRIP_W - theme.space_md).max(100.0);

            // Channel strips inside horizontal ScrollArea
            ScrollArea::horizontal()
                .id_source("sys_mixer_scroll")
                .max_width(scroll_width)
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        let num_ch = app.mixer.num_channels.clamp(1, 16);
                        for i in 0..num_ch {
                            render_channel_strip_full(app, ui, i, telemetry, true, false);
                            if app.mixer.folded_pads[i] {
                                ui.add_space(theme.space_xs);
                                render_pad_subchannel_strips(app, ui, i);
                            }
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
                render_master_strip_full(app, ui, telemetry, false);
            });
        });
    });
}

/// Standalone collapsible/detachable bottom mixer drawer for DJ Console & Composer views.
pub fn render_mixer_drawer(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>, show_crossfader: bool) {
    let theme = app.theme;

    if show_crossfader {
        // DJ Console A/B Crossfader Bar inside drawer
        ui.horizontal(|ui| {
            ui.label(RichText::new("CROSSFADER A").strong().color(theme.deck_colors[0]).size(theme.type_caption));
            ui.add_space(theme.space_sm);
            widgets::render_horizontal_fader(ui, &mut app.mixer.crossfader_pos, 0.0..=1.0, theme.accent, 220.0, 16.0);
            ui.add_space(theme.space_sm);
            ui.label(RichText::new("CROSSFADER B").strong().color(theme.deck_colors[1]).size(theme.type_caption));
        });
        ui.add_space(theme.space_xs);
    }

    ui.horizontal_top(|ui| {
        let scroll_width = (ui.available_width() - STRIP_W - theme.space_md).max(100.0);

        ScrollArea::horizontal()
            .id_source("drawer_mixer_scroll")
            .max_width(scroll_width)
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let num_ch = app.mixer.num_channels.clamp(1, 16);
                    for i in 0..num_ch {
                        render_channel_strip_full(app, ui, i, telemetry, show_crossfader, true);
                        if app.mixer.folded_pads[i] {
                            ui.add_space(theme.space_xs);
                            render_pad_subchannel_strips(app, ui, i);
                        }
                        ui.add_space(theme.space_xs);
                    }
                });
            });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            render_master_strip_full(app, ui, telemetry, true);
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
                    .min_size(Vec2::new(40.0, 40.0));
                if ui.add(btn).on_hover_text("Add Channel Strip").clicked() {
                    if app.mixer.num_channels < 16 {
                        app.mixer.num_channels += 1;
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new("ADD CHANNEL").size(8.0).strong().color(theme.text_secondary));
            });
        });
}

fn render_channel_fx_rack_item(app: &mut InspectorApp, ui: &mut Ui, ch_idx: usize, fx_idx: usize, accent_color: Color32) {
    ui.push_id(("ch_fx", ch_idx, fx_idx), |ui| {
        let theme = app.theme;
        let active_deck = ch_idx % 4;
        let insert_name = app.decks.deck_inserts[active_deck][fx_idx].clone();
        let num_inserts = app.decks.deck_inserts[active_deck].len();

        Frame::none()
            .fill(theme.bg_inset)
            .rounding(Rounding::same(theme.radius_sm))
            .inner_margin(Margin::same(2.0))
            .stroke(Stroke::new(1.0, theme.border))
            .show(ui, |ui| {
                ui.set_width(STRIP_W - 12.0);
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(&insert_name).size(7.5).strong().color(accent_color)).truncate());

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Remove button
                            if ui.add(egui::Button::new(RichText::new("✕").size(7.0)).fill(theme.bg_inset)).clicked() {
                                app.decks.deck_inserts[active_deck].remove(fx_idx);
                                if fx_idx < app.decks.deck_insert_params[active_deck].len() {
                                    app.decks.deck_insert_params[active_deck].remove(fx_idx);
                                }
                                return;
                            }

                            // Reorder Down
                            if fx_idx + 1 < num_inserts {
                                if ui.add(egui::Button::new(RichText::new("▼").size(7.0)).fill(theme.bg_inset)).clicked() {
                                    app.decks.deck_inserts[active_deck].swap(fx_idx, fx_idx + 1);
                                    if fx_idx + 1 < app.decks.deck_insert_params[active_deck].len() {
                                        app.decks.deck_insert_params[active_deck].swap(fx_idx, fx_idx + 1);
                                    }
                                    return;
                                }
                            }

                            // Reorder Up
                            if fx_idx > 0 {
                                if ui.add(egui::Button::new(RichText::new("▲").size(7.0)).fill(theme.bg_inset)).clicked() {
                                    app.decks.deck_inserts[active_deck].swap(fx_idx, fx_idx - 1);
                                    if fx_idx < app.decks.deck_insert_params[active_deck].len() {
                                        app.decks.deck_insert_params[active_deck].swap(fx_idx, fx_idx - 1);
                                    }
                                    return;
                                }
                            }
                        });
                    });

                    ui.add_space(1.0);

                    let name_upper = insert_name.to_uppercase();
                    if name_upper.contains("TRIM") || name_upper.contains("GAIN") {
                        let gain_node = app.topo.node_map.get(&format!("deck_{}_gain", (b'a' + active_deck as u8) as char)).copied();
                        let mut gain_val = app.mixer.channel_gain[ch_idx];
                        if widgets::render_knob_sized(ui, &mut gain_val, 0.0..=2.0, "GAIN", accent_color, 18.0).changed() {
                            app.mixer.channel_gain[ch_idx] = gain_val;
                            if let Some(gain_id) = gain_node {
                                let net_gain = gain_val * app.mixer.channel_faders[ch_idx];
                                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: gain_id as u64,
                                    param_id: 0,
                                    value: net_gain,
                                    ramp_duration_samples: 128,
                                }));
                            }
                        }
                    } else if name_upper.contains("PITCH") || name_upper.contains("SPEED") {
                        let mut pitch_val = app.mixer.channel_pitch[ch_idx];
                        if widgets::render_knob_sized(ui, &mut pitch_val, 0.5..=2.0, "PITCH", accent_color, 18.0).changed() {
                            app.mixer.channel_pitch[ch_idx] = pitch_val;
                        }
                    } else if name_upper.contains("3-BAND") || name_upper.contains("EQ") || name_upper.contains("ISOLATOR") {
                        let iso_node = app.topo.node_map.get(&format!("deck_{}_isolator", (b'a' + active_deck as u8) as char)).copied();
                        let mut hi = app.mixer.channel_eq_high[ch_idx];
                        if widgets::render_knob_sized(ui, &mut hi, 0.0..=2.0, "HI", accent_color, 18.0).changed() {
                            app.mixer.channel_eq_high[ch_idx] = hi;
                            if let Some(node_id) = iso_node {
                                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: node_id as u64,
                                    param_id: 2,
                                    value: hi,
                                    ramp_duration_samples: 128,
                                }));
                            }
                        }
                        ui.add_space(1.0);
                        let mut mid = app.mixer.channel_eq_mid[ch_idx];
                        if widgets::render_knob_sized(ui, &mut mid, 0.0..=2.0, "MID", accent_color, 18.0).changed() {
                            app.mixer.channel_eq_mid[ch_idx] = mid;
                            if let Some(node_id) = iso_node {
                                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: node_id as u64,
                                    param_id: 1,
                                    value: mid,
                                    ramp_duration_samples: 128,
                                }));
                            }
                        }
                        ui.add_space(1.0);
                        let mut low = app.mixer.channel_eq_low[ch_idx];
                        if widgets::render_knob_sized(ui, &mut low, 0.0..=2.0, "LOW", accent_color, 18.0).changed() {
                            app.mixer.channel_eq_low[ch_idx] = low;
                            if let Some(node_id) = iso_node {
                                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: node_id as u64,
                                    param_id: 0,
                                    value: low,
                                    ramp_duration_samples: 128,
                                }));
                            }
                        }
                    } else {
                        if let Some(params) = app.decks.deck_insert_params[active_deck].get_mut(fx_idx) {
                            widgets::render_knob_sized(ui, &mut params[0], 0.0..=1.0, "MIX", accent_color, 18.0);
                        }
                    }
                });
            });
    });
}

fn render_sampler_subchannel_fx_item(app: &mut InspectorApp, ui: &mut Ui, pad_idx: usize, fx_idx: usize, accent_color: Color32) {
    ui.push_id(("sampler_fx", pad_idx, fx_idx), |ui| {
        let theme = app.theme;
        let insert_name = app.sampler.subchannel_inserts[pad_idx][fx_idx].clone();
        let num_inserts = app.sampler.subchannel_inserts[pad_idx].len();

        Frame::none()
            .fill(theme.bg_inset)
            .rounding(Rounding::same(theme.radius_sm))
            .inner_margin(Margin::same(2.0))
            .stroke(Stroke::new(1.0, theme.border))
            .show(ui, |ui| {
                ui.set_width(STRIP_W - 12.0);
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(&insert_name).size(7.5).strong().color(accent_color)).truncate());

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Remove button
                            if ui.add(egui::Button::new(RichText::new("✕").size(7.0)).fill(theme.bg_inset)).clicked() {
                                app.sampler.subchannel_inserts[pad_idx].remove(fx_idx);
                                if fx_idx < app.sampler.subchannel_insert_params[pad_idx].len() {
                                    app.sampler.subchannel_insert_params[pad_idx].remove(fx_idx);
                                }
                                return;
                            }

                            // Reorder Down
                            if fx_idx + 1 < num_inserts {
                                if ui.add(egui::Button::new(RichText::new("▼").size(7.0)).fill(theme.bg_inset)).clicked() {
                                    app.sampler.subchannel_inserts[pad_idx].swap(fx_idx, fx_idx + 1);
                                    if fx_idx + 1 < app.sampler.subchannel_insert_params[pad_idx].len() {
                                        app.sampler.subchannel_insert_params[pad_idx].swap(fx_idx, fx_idx + 1);
                                    }
                                    return;
                                }
                            }

                            // Reorder Up
                            if fx_idx > 0 {
                                if ui.add(egui::Button::new(RichText::new("▲").size(7.0)).fill(theme.bg_inset)).clicked() {
                                    app.sampler.subchannel_inserts[pad_idx].swap(fx_idx, fx_idx - 1);
                                    if fx_idx < app.sampler.subchannel_insert_params[pad_idx].len() {
                                        app.sampler.subchannel_insert_params[pad_idx].swap(fx_idx, fx_idx - 1);
                                    }
                                    return;
                                }
                            }
                        });
                    });

                    ui.add_space(1.0);

                    let name_upper = insert_name.to_uppercase();
                    if name_upper.contains("TRIM") || name_upper.contains("GAIN") {
                        let mut gain = app.sampler.subchannel_gain[pad_idx];
                        if widgets::render_knob_sized(ui, &mut gain, 0.0..=2.0, "GAIN", accent_color, 18.0).changed() {
                            app.sampler.subchannel_gain[pad_idx] = gain;
                        }
                    } else if name_upper.contains("PITCH") || name_upper.contains("TUNE") {
                        let mut pitch = app.sampler.pad_tune[pad_idx];
                        if widgets::render_knob_sized(ui, &mut pitch, 0.0..=1.0, "PITCH", accent_color, 18.0).changed() {
                            app.sampler.pad_tune[pad_idx] = pitch;
                        }
                    } else if name_upper.contains("3-BAND") || name_upper.contains("EQ") {
                        let mut hi = app.sampler.subchannel_eq_high[pad_idx];
                        if widgets::render_knob_sized(ui, &mut hi, 0.0..=2.0, "HI", accent_color, 18.0).changed() {
                            app.sampler.subchannel_eq_high[pad_idx] = hi;
                        }
                        ui.add_space(1.0);
                        let mut mid = app.sampler.subchannel_eq_mid[pad_idx];
                        if widgets::render_knob_sized(ui, &mut mid, 0.0..=2.0, "MID", accent_color, 18.0).changed() {
                            app.sampler.subchannel_eq_mid[pad_idx] = mid;
                        }
                        ui.add_space(1.0);
                        let mut low = app.sampler.subchannel_eq_low[pad_idx];
                        if widgets::render_knob_sized(ui, &mut low, 0.0..=2.0, "LOW", accent_color, 18.0).changed() {
                            app.sampler.subchannel_eq_low[pad_idx] = low;
                        }
                    } else {
                        if let Some(params) = app.sampler.subchannel_insert_params[pad_idx].get_mut(fx_idx) {
                            widgets::render_knob_sized(ui, &mut params[0], 0.0..=1.0, "MIX", accent_color, 18.0);
                        }
                    }
                });
            });
    });
}

fn render_custom_subchannel_fx_item(app: &mut InspectorApp, ui: &mut Ui, parent_ch: usize, sub_idx: usize, fx_idx: usize, accent_color: Color32) {
    ui.push_id(("custom_fx", parent_ch, sub_idx, fx_idx), |ui| {
        let theme = app.theme;
        let insert_name = app.mixer.custom_subchannel_inserts[parent_ch][sub_idx][fx_idx].clone();
        let num_inserts = app.mixer.custom_subchannel_inserts[parent_ch][sub_idx].len();

        Frame::none()
            .fill(theme.bg_inset)
            .rounding(Rounding::same(theme.radius_sm))
            .inner_margin(Margin::same(2.0))
            .stroke(Stroke::new(1.0, theme.border))
            .show(ui, |ui| {
                ui.set_width(STRIP_W - 12.0);
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(&insert_name).size(7.5).strong().color(accent_color)).truncate());

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Remove button
                            if ui.add(egui::Button::new(RichText::new("✕").size(7.0)).fill(theme.bg_inset)).clicked() {
                                app.mixer.custom_subchannel_inserts[parent_ch][sub_idx].remove(fx_idx);
                                if fx_idx < app.mixer.custom_subchannel_insert_params[parent_ch][sub_idx].len() {
                                    app.mixer.custom_subchannel_insert_params[parent_ch][sub_idx].remove(fx_idx);
                                }
                                return;
                            }

                            // Reorder Down
                            if fx_idx + 1 < num_inserts {
                                if ui.add(egui::Button::new(RichText::new("▼").size(7.0)).fill(theme.bg_inset)).clicked() {
                                    app.mixer.custom_subchannel_inserts[parent_ch][sub_idx].swap(fx_idx, fx_idx + 1);
                                    if fx_idx + 1 < app.mixer.custom_subchannel_insert_params[parent_ch][sub_idx].len() {
                                        app.mixer.custom_subchannel_insert_params[parent_ch][sub_idx].swap(fx_idx, fx_idx + 1);
                                    }
                                    return;
                                }
                            }

                            // Reorder Up
                            if fx_idx > 0 {
                                if ui.add(egui::Button::new(RichText::new("▲").size(7.0)).fill(theme.bg_inset)).clicked() {
                                    app.mixer.custom_subchannel_inserts[parent_ch][sub_idx].swap(fx_idx, fx_idx - 1);
                                    if fx_idx < app.mixer.custom_subchannel_insert_params[parent_ch][sub_idx].len() {
                                        app.mixer.custom_subchannel_insert_params[parent_ch][sub_idx].swap(fx_idx, fx_idx - 1);
                                    }
                                    return;
                                }
                            }
                        });
                    });

                    ui.add_space(1.0);

                    let name_upper = insert_name.to_uppercase();
                    if name_upper.contains("TRIM") || name_upper.contains("GAIN") {
                        let mut gain = app.mixer.custom_subchannel_gain[parent_ch][sub_idx];
                        if widgets::render_knob_sized(ui, &mut gain, 0.0..=2.0, "GAIN", accent_color, 18.0).changed() {
                            app.mixer.custom_subchannel_gain[parent_ch][sub_idx] = gain;
                        }
                    } else if name_upper.contains("PITCH") {
                        let mut pitch = app.mixer.custom_subchannel_pitch[parent_ch][sub_idx];
                        if widgets::render_knob_sized(ui, &mut pitch, 0.5..=2.0, "PITCH", accent_color, 18.0).changed() {
                            app.mixer.custom_subchannel_pitch[parent_ch][sub_idx] = pitch;
                        }
                    } else if name_upper.contains("3-BAND") || name_upper.contains("EQ") {
                        let mut hi = app.mixer.custom_subchannel_eq_high[parent_ch][sub_idx];
                        if widgets::render_knob_sized(ui, &mut hi, 0.0..=2.0, "HI", accent_color, 18.0).changed() {
                            app.mixer.custom_subchannel_eq_high[parent_ch][sub_idx] = hi;
                        }
                        ui.add_space(1.0);
                        let mut mid = app.mixer.custom_subchannel_eq_mid[parent_ch][sub_idx];
                        if widgets::render_knob_sized(ui, &mut mid, 0.0..=2.0, "MID", accent_color, 18.0).changed() {
                            app.mixer.custom_subchannel_eq_mid[parent_ch][sub_idx] = mid;
                        }
                        ui.add_space(1.0);
                        let mut low = app.mixer.custom_subchannel_eq_low[parent_ch][sub_idx];
                        if widgets::render_knob_sized(ui, &mut low, 0.0..=2.0, "LOW", accent_color, 18.0).changed() {
                            app.mixer.custom_subchannel_eq_low[parent_ch][sub_idx] = low;
                        }
                    } else {
                        if let Some(params) = app.mixer.custom_subchannel_insert_params[parent_ch][sub_idx].get_mut(fx_idx) {
                            widgets::render_knob_sized(ui, &mut params[0], 0.0..=1.0, "MIX", accent_color, 18.0);
                        }
                    }
                });
            });
    });
}

const NEEDLE_WINDOW_SECS: f32 = 8.0;

fn render_vertical_waveform(
    app: &mut InspectorApp,
    ui: &mut Ui,
    deck_idx: usize,
    raw_elapsed: u64,
    peak_level: f32,
    deck_color: Color32,
    theme: &nullherz_ui_hal::Theme,
    telemetry: &Option<Telemetry>,
) {
    let wf_w = ui.available_width();
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(wf_w, VERTICAL_WAVEFORM_H), egui::Sense::hover());

    ui.painter().rect_filled(rect, theme.radius_sm, theme.bg_inset);
    ui.painter().rect_stroke(rect, theme.radius_sm, Stroke::new(1.0_f32, theme.border_stroke.color));

    let style = app.mixer.waveform_styles.get(deck_idx).copied().unwrap_or(nullherz_ui_hal::render::waveform_renderer::WaveformStyle::MultiBand);
    let track = if app.active_view == crate::View::Composer {
        app.composer.track_sources.get(deck_idx)
            .and_then(|opt| *opt)
            .and_then(|id| app.get_cached_track(id))
    } else if deck_idx < 4 {
        app.decks.cached_tracks.get(deck_idx).and_then(|t| t.clone())
    } else {
        app.composer.track_sources.get(deck_idx)
            .and_then(|opt| *opt)
            .and_then(|id| app.get_cached_track(id))
    };

    if let Some(t) = track {
        let sr = (t.metadata.sample_rate.max(1)) as f32;
        let total_frames = t.metadata.total_samples.max(1);
        let active_deck = deck_idx % 4;
        let is_playing = app.decks.deck_playing[active_deck];
        let playback_rate = telemetry.as_ref().map(|tel| tel.deck_playback_rates[active_deck]).unwrap_or(1.0);

        let elapsed_samples = if is_playing && telemetry.is_some() {
            let now = ui.input(|inp| inp.time);
            let dt = (now - app.last_telemetry_time).max(0.0) as f32;
            let interp_frames = (dt * playback_rate * sr) as u64;
            (raw_elapsed + interp_frames).min(total_frames)
        } else {
            raw_elapsed
        };

        let window_frames = NEEDLE_WINDOW_SECS * sr * (playback_rate as f32).max(0.01);
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
                            Stroke::new(2.0_f32, col),
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
                            Stroke::new(2.0_f32, deck_color.linear_multiply(0.8)),
                        );
                    }
                }
            }
        }

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
            Stroke::new(2.0_f32, deck_color.linear_multiply(0.6)),
        );
    }
}

#[allow(dead_code)]
pub fn render_channel_strip(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    render_channel_strip_full(app, ui, i, telemetry, false, false);
}

#[allow(dead_code)]
pub fn render_channel_strip_ext(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>, show_crossfader: bool) {
    render_channel_strip_full(app, ui, i, telemetry, show_crossfader, false);
}

pub fn render_channel_strip_full(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>, show_crossfader: bool, in_drawer: bool) {
    ui.push_id(i, |ui| {
        let theme = app.theme;
        let deck_color = crate::InspectorApp::deck_color(&theme, i % 4);
        let deck_char_letter = (b'a' + (i % 26) as u8) as char;

        let gain_node = app.topo.node_map.get(&format!("deck_{}_gain", deck_char_letter)).copied();
        let iso_node = app.topo.node_map.get(&format!("deck_{}_isolator", deck_char_letter)).copied();
        let meter_node = iso_node.or_else(|| app.topo.node_map.get(&format!("deck_{}_sampler", deck_char_letter)).copied());

        let level_base = if let (Some(t), Some(node)) = (telemetry, meter_node) {
            t.peak_levels.get(node as usize).copied().unwrap_or(0.0)
        } else {
            0.0
        };

        let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions.get(i % 4).copied().unwrap_or(0)).unwrap_or(0);
        let is_focused = app.decks.focused_deck == i;

        let border_stroke = if is_focused {
            Stroke::new(2.0_f32, deck_color)
        } else {
            Stroke::new(1.0_f32, theme.border)
        };

        let fill_color = if is_focused {
            theme.bg_surface_raised
        } else {
            theme.bg_surface
        };

        let is_detached = !in_drawer && app.detached_views.contains(&crate::View::Mixer);

        ui.horizontal(|ui| {
            Frame::none()
                .fill(fill_color)
                .rounding(Rounding::same(theme.radius_md))
                .inner_margin(Margin::same(4.0))
                .stroke(border_stroke)
                .show(ui, |ui| {
                    ui.set_width(STRIP_W);
                    ui.vertical(|ui| {
                        let is_drum_machine = app.mixer.channel_input_sources[i] == ChannelInputSource::DrumMachine;
                        let strip_accent = if is_drum_machine { Color32::from_rgb(255, 127, 62) } else { deck_color };

                        let mut fold_clicked = false;
                        let mut remove_ch_clicked = false;
                        let header_resp = ui.horizontal(|ui| {
                            let fold_icon = if app.mixer.folded_pads[i] { "▼" } else { "▸" };
                            if ui.button(RichText::new(fold_icon).size(9.0).color(strip_accent)).on_hover_text("Toggle inline subchannels").clicked() {
                                fold_clicked = true;
                            }

                            ui.label(RichText::new(format!("CH {}", (b'A' + (i % 26) as u8) as char)).strong().size(theme.type_body).color(strip_accent));

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if app.mixer.num_channels > 1 {
                                    if ui.add(egui::Button::new(RichText::new("✕").size(7.5)).fill(theme.bg_inset)).on_hover_text("Remove Channel").clicked() {
                                        remove_ch_clicked = true;
                                    }
                                }
                                if ui.add(egui::Button::new(RichText::new("🔍").size(7.5)).fill(theme.bg_inset)).on_hover_text("Open Channel Inspector").clicked() {
                                    app.mixer.focused_detail_channel = i;
                                    app.active_view = crate::View::ChannelDetail;
                                }
                            });
                        });

                        if remove_ch_clicked {
                            if app.mixer.num_channels > 1 {
                                app.mixer.num_channels -= 1;
                            }
                            return;
                        }

                        if fold_clicked {
                            app.mixer.folded_pads[i] = !app.mixer.folded_pads[i];
                        } else if header_resp.response.interact(egui::Sense::click()).clicked() {
                            app.decks.focused_deck = i;
                        }
                        ui.add_space(2.0);

                        if is_detached {
                            // Waveform Style Dropdown Selector
                            ui.horizontal(|ui| {
                                let avail_w = ui.available_width();
                                let selected_style = app.mixer.waveform_styles[i];
                                egui::ComboBox::from_id_source(format!("ch_wf_style_{}", i))
                                    .selected_text(RichText::new(selected_style.name()).size(8.0).strong().color(theme.text_primary))
                                    .width(avail_w)
                                    .show_ui(ui, |ui| {
                                        for st in nullherz_ui_hal::render::waveform_renderer::WaveformStyle::all() {
                                            ui.selectable_value(&mut app.mixer.waveform_styles[i], *st, st.name());
                                        }
                                    });
                            });
                            ui.add_space(2.0);

                            // Vertical Waveform Canvas (Only rendered in detached window)
                            render_vertical_waveform(app, ui, i, elapsed_samples, level_base, deck_color, &theme, telemetry);
                            ui.add_space(2.0);
                        }

                        // Centered Column Controls Group: SORTABLE ROTARY FX RACK
                        ui.group(|ui| {
                            ui.set_width(STRIP_W - 8.0);
                            ui.vertical_centered(|ui| {
                                let fx_count = app.decks.deck_inserts[i % 4].len();
                                for fx_i in 0..fx_count {
                                    render_channel_fx_rack_item(app, ui, i, fx_i, strip_accent);
                                    ui.add_space(2.0);
                                }

                                if ui.add_sized([STRIP_W - 12.0, 16.0], egui::Button::new(RichText::new("+ FX").size(8.0).strong()).fill(theme.bg_inset)).clicked() {
                                    app.decks.deck_inserts[i % 4].push("CUSTOM INSERT FX".into());
                                    app.decks.deck_insert_params[i % 4].push([1.0; 8]);
                                    app.active_right_tab = Some(crate::RightTab::Store);
                                    app.store.active_category = Some(sidecar_sdk::AssetCategory::AudioInsert);
                                }
                            });
                        });

                        ui.add_space( theme.space_sm);

                        // --- VOLUME FADER & STEREO VU METERS ---
                        ui.horizontal(|ui| {
                            let fader_w = 20.0;
                            let pad = (ui.available_width() - fader_w).max(0.0) / 2.0;
                            ui.add_space(pad);

                            let r_fader = widgets::render_fader(ui, &mut app.mixer.channel_faders[i], 0.0..=1.2, deck_color, FADER_H, 22.0);
                            if r_fader.changed() {
                                if let Some(gain_id) = gain_node {
                                    let net_gain = app.mixer.channel_gain[i] * app.mixer.channel_faders[i];
                                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                        target_id: gain_id as u64,
                                        param_id: 0,
                                        value: net_gain,
                                        ramp_duration_samples: 128,
                                    }));
                                }
                            }

                            ui.add_space(2.0);

                            // STEREO VU METERS
                            if let (Some(t), Some(node)) = (telemetry, meter_node) {
                                let level_base = t.peak_levels.get(node as usize).copied().unwrap_or(0.0);
                                let bal = app.mixer.channel_balance[i];
                                let level_l = level_base * (1.0 - (bal - 0.5).max(0.0));
                                let level_r = level_base * (1.0 - (0.5 - bal).max(0.0));

                                widgets::render_vu_meter(ui, level_l, app.mixer.channel_peak_hold[i], deck_color, FADER_H);
                                ui.add_space(1.0);
                                widgets::render_vu_meter(ui, level_r, app.mixer.channel_peak_hold[i], deck_color, FADER_H);
                            } else {
                                widgets::render_vu_meter(ui, 0.0, 0.0, theme.text_disabled, FADER_H);
                                ui.add_space(1.0);
                                widgets::render_vu_meter(ui, 0.0, 0.0, theme.text_disabled, FADER_H);
                            }
                        });

                        ui.add_space(2.0);

                        // Crossfader Assignment Selector (A / OFF / B)
                        if show_crossfader {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 1.0;
                                let assign = app.mixer.crossfader_assign[i];

                                let a_btn = ui.selectable_label(assign == 0, RichText::new("A").size(7.5).strong());
                                if a_btn.clicked() { app.mixer.crossfader_assign[i] = 0; }

                                let thru_btn = ui.selectable_label(assign == 1, RichText::new("OFF").size(7.5).strong());
                                if thru_btn.clicked() { app.mixer.crossfader_assign[i] = 1; }

                                let b_btn = ui.selectable_label(assign == 2, RichText::new("B").size(7.5).strong());
                                if b_btn.clicked() { app.mixer.crossfader_assign[i] = 2; }
                            });
                            ui.add_space(2.0);
                        }

                        // Input Source Selector
                        ui.horizontal(|ui| {
                            let selected_source = app.mixer.channel_input_sources[i];
                            egui::ComboBox::from_id_source(format!("ch_input_src_{}", i))
                                .selected_text(RichText::new(selected_source.name()).size(8.0).strong().color(theme.text_primary))
                                .width(STRIP_W - 12.0)
                                .show_ui(ui, |ui| {
                                    for src in ChannelInputSource::all() {
                                        ui.selectable_value(&mut app.mixer.channel_input_sources[i], *src, src.name());
                                    }
                                });
                        });
                        ui.add_space(2.0);

                        // Transport Row
                        ui.horizontal(|ui| {
                            let active_deck_idx = i % 4;
                            let is_playing = app.decks.deck_playing[active_deck_idx];
                            let play_icon = if is_playing { egui_phosphor::regular::PAUSE } else { egui_phosphor::regular::PLAY };
                            let play_btn = if is_playing {
                                egui::Button::new(RichText::new(play_icon).size(10.0).strong()).fill(theme.accent)
                            } else {
                                egui::Button::new(RichText::new(play_icon).size(10.0).strong()).fill(theme.bg_inset)
                            };

                            let deck_char_upper = (b'A' + active_deck_idx as u8) as char;
                            if ui.add_sized([34.0, 18.0], play_btn).clicked() {
                                app.decks.focused_deck = active_deck_idx;
                                let new_playing = !is_playing;
                                app.decks.deck_playing[active_deck_idx] = new_playing;
                                if new_playing {
                                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::PlayDeck { deck_id: deck_char_upper }));
                                } else {
                                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::StopDeck { deck_id: deck_char_upper }));
                                }
                            }

                            if ui.add_sized([34.0, 18.0], egui::Button::new(RichText::new("CUE").size(8.5).strong()).fill(theme.bg_inset)).clicked() {
                                app.decks.focused_deck = active_deck_idx;
                                let node_name = format!("deck_{}_sampler", (b'a' + active_deck_idx as u8) as char);
                                if let Some(node_idx) = app.get_node_id(&node_name) {
                                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpToHotCue { node_idx, cue_idx: 0 }));
                                }
                            }
                        });
                    });
                });

        });
    });
}

pub fn render_pad_subchannel_strips(app: &mut InspectorApp, ui: &mut Ui, parent_ch: usize) {
    let theme = app.theme;
    let subchannel_color = Color32::from_rgb(255, 127, 62); // Warm drum accent (#FF7F3E)

    let pad_names = [
        "KICK", "SNARE", "HH-CL", "HH-OP", "TOM-L", "TOM-M", "TOM-H", "PERC1",
        "PERC2", "CLAP", "RIDE", "CRASH", "FX1", "FX2", "AUX1", "AUX2",
    ];

    let is_drum_machine = app.mixer.channel_input_sources[parent_ch] == ChannelInputSource::DrumMachine;

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        // Render Drum Machine Subchannels if Drum Machine source
        if is_drum_machine {
            for pad_idx in 0..16 {
                ui.push_id((parent_ch, pad_idx), |ui| {
                    Frame::none()
                        .fill(theme.bg_surface)
                        .rounding(Rounding::same(theme.radius_md))
                        .inner_margin(Margin::same(4.0))
                        .stroke(Stroke::new(1.0, subchannel_color.linear_multiply(0.7)))
                        .show(ui, |ui| {
                            ui.set_width(STRIP_W);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("↳{:02}", pad_idx + 1)).size(theme.type_caption).strong().color(subchannel_color));
                                    ui.label(RichText::new(pad_names[pad_idx]).size(8.0).strong().color(theme.text_primary));
                                });
                                ui.add_space(2.0);

                                // Subchannel Centered Controls Group
                                ui.group(|ui| {
                                    ui.set_width(STRIP_W - 8.0);
                                    ui.vertical_centered(|ui| {
                                        // FX Rack for subchannel
                                        let fx_count = app.sampler.subchannel_inserts[pad_idx].len();
                                        for fx_i in 0..fx_count {
                                            render_sampler_subchannel_fx_item(app, ui, pad_idx, fx_i, subchannel_color);
                                            ui.add_space(2.0);
                                        }

                                        if ui.add_sized([STRIP_W - 12.0, 16.0], egui::Button::new(RichText::new("+ FX").size(8.0).strong()).fill(theme.bg_inset)).clicked() {
                                            app.sampler.subchannel_inserts[pad_idx].push("CUSTOM INSERT FX".into());
                                            app.sampler.subchannel_insert_params[pad_idx].push([1.0; 8]);
                                            app.active_right_tab = Some(crate::RightTab::Store);
                                            app.store.active_category = Some(sidecar_sdk::AssetCategory::AudioInsert);
                                        }
                                    });
                                });

                                ui.add_space(theme.space_sm);

                                // Volume Fader & Mute/Solo
                                ui.horizontal(|ui| {
                                    let mut pad_fader = app.sampler.subchannel_faders[pad_idx];
                                    if widgets::render_fader(ui, &mut pad_fader, 0.0..=1.2, subchannel_color, FADER_H, 20.0).changed() {
                                        app.sampler.subchannel_faders[pad_idx] = pad_fader;
                                    }

                                    ui.add_space(2.0);

                                    ui.vertical(|ui| {
                                        let is_muted = app.sampler.subchannel_mutes[pad_idx];
                                        let mute_color = if is_muted { theme.danger } else { theme.bg_inset };
                                        if ui.add_sized([18.0, 18.0], egui::Button::new(RichText::new("M").size(7.5).strong()).fill(mute_color)).clicked() {
                                            app.sampler.subchannel_mutes[pad_idx] = !is_muted;
                                        }

                                        ui.add_space(2.0);

                                        let is_solo = app.sampler.subchannel_solos[pad_idx];
                                        let solo_color = if is_solo { theme.warning } else { theme.bg_inset };
                                        if ui.add_sized([18.0, 18.0], egui::Button::new(RichText::new("S").size(7.5).strong()).fill(solo_color)).clicked() {
                                            app.sampler.subchannel_solos[pad_idx] = !is_solo;
                                        }
                                    });
                                });
                            });
                        });
                });
            }
        }

        // Render Custom User Subchannels (for non-instrument or instrument channels)
        let custom_count = app.mixer.custom_subchannels[parent_ch].len();
        let mut remove_sub_idx: Option<usize> = None;
        for sub_idx in 0..custom_count {
            let sub_name = app.mixer.custom_subchannels[parent_ch][sub_idx].clone();
            ui.push_id((parent_ch, 100 + sub_idx), |ui| {
                Frame::none()
                    .fill(theme.bg_surface)
                    .rounding(Rounding::same(theme.radius_md))
                    .inner_margin(Margin::same(4.0))
                    .stroke(Stroke::new(1.0, subchannel_color.linear_multiply(0.7)))
                    .show(ui, |ui| {
                        ui.set_width(STRIP_W);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("↳{:02}", sub_idx + 1)).size(theme.type_caption).strong().color(subchannel_color));
                                ui.label(RichText::new(&sub_name).size(8.0).strong().color(theme.text_primary));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.add(egui::Button::new(RichText::new("✕").size(7.5)).fill(theme.bg_inset)).on_hover_text("Remove Subchannel").clicked() {
                                        remove_sub_idx = Some(sub_idx);
                                    }
                                });
                            });
                            ui.add_space(2.0);

                            ui.group(|ui| {
                                ui.set_width(STRIP_W - 8.0);
                                ui.vertical_centered(|ui| {
                                    // Custom Subchannel Sortable FX Rack
                                    let fx_count = app.mixer.custom_subchannel_inserts[parent_ch][sub_idx].len();
                                    for fx_i in 0..fx_count {
                                        render_custom_subchannel_fx_item(app, ui, parent_ch, sub_idx, fx_i, subchannel_color);
                                        ui.add_space(2.0);
                                    }

                                    if ui.add_sized([STRIP_W - 12.0, 16.0], egui::Button::new(RichText::new("+ FX").size(8.0).strong()).fill(theme.bg_inset)).clicked() {
                                        app.mixer.custom_subchannel_inserts[parent_ch][sub_idx].push("CUSTOM INSERT FX".into());
                                        app.mixer.custom_subchannel_insert_params[parent_ch][sub_idx].push([1.0; 8]);
                                        app.active_right_tab = Some(crate::RightTab::Store);
                                        app.store.active_category = Some(sidecar_sdk::AssetCategory::AudioInsert);
                                    }
                                });
                            });

                            ui.add_space(theme.space_sm);

                            ui.horizontal(|ui| {
                                let mut fader = app.mixer.custom_subchannel_faders[parent_ch][sub_idx];
                                if widgets::render_fader(ui, &mut fader, 0.0..=1.2, subchannel_color, FADER_H, 20.0).changed() {
                                    app.mixer.custom_subchannel_faders[parent_ch][sub_idx] = fader;
                                }

                                ui.add_space(2.0);

                                ui.vertical(|ui| {
                                    let is_muted = app.mixer.custom_subchannel_mutes[parent_ch][sub_idx];
                                    let mute_color = if is_muted { theme.danger } else { theme.bg_inset };
                                    if ui.add_sized([18.0, 18.0], egui::Button::new(RichText::new("M").size(7.5).strong()).fill(mute_color)).clicked() {
                                        app.mixer.custom_subchannel_mutes[parent_ch][sub_idx] = !is_muted;
                                    }

                                    ui.add_space(2.0);

                                    let is_solo = app.mixer.custom_subchannel_solos[parent_ch][sub_idx];
                                    let solo_color = if is_solo { theme.warning } else { theme.bg_inset };
                                    if ui.add_sized([18.0, 18.0], egui::Button::new(RichText::new("S").size(7.5).strong()).fill(solo_color)).clicked() {
                                        app.mixer.custom_subchannel_solos[parent_ch][sub_idx] = !is_solo;
                                    }
                                });
                            });
                        });
                    });
            });
        }

        if let Some(sub_idx) = remove_sub_idx {
            if sub_idx < app.mixer.custom_subchannels[parent_ch].len() {
                app.mixer.custom_subchannels[parent_ch].remove(sub_idx);
                app.mixer.custom_subchannel_gain[parent_ch].remove(sub_idx);
                app.mixer.custom_subchannel_pitch[parent_ch].remove(sub_idx);
                app.mixer.custom_subchannel_eq_high[parent_ch].remove(sub_idx);
                app.mixer.custom_subchannel_eq_mid[parent_ch].remove(sub_idx);
                app.mixer.custom_subchannel_eq_low[parent_ch].remove(sub_idx);
                app.mixer.custom_subchannel_faders[parent_ch].remove(sub_idx);
                app.mixer.custom_subchannel_mutes[parent_ch].remove(sub_idx);
                app.mixer.custom_subchannel_solos[parent_ch].remove(sub_idx);
                if sub_idx < app.mixer.custom_subchannel_inserts[parent_ch].len() {
                    app.mixer.custom_subchannel_inserts[parent_ch].remove(sub_idx);
                    app.mixer.custom_subchannel_insert_params[parent_ch].remove(sub_idx);
                }
            }
        }

        // Add Subchannel Button Card
        Frame::none()
            .fill(theme.bg_surface)
            .rounding(Rounding::same(theme.radius_md))
            .inner_margin(Margin::same(4.0))
            .stroke(Stroke::new(1.0_f32, subchannel_color.linear_multiply(0.5)))
            .show(ui, |ui| {
                ui.set_width(STRIP_W);
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    let btn = egui::Button::new(RichText::new("+").size(18.0).strong().color(subchannel_color))
                        .fill(theme.bg_inset)
                        .min_size(Vec2::new(36.0, 36.0));
                    if ui.add(btn).on_hover_text("Add Custom Subchannel").clicked() {
                        let new_sub_num = app.mixer.custom_subchannels[parent_ch].len() + 1;
                        app.mixer.custom_subchannels[parent_ch].push(format!("SUB {:02}", new_sub_num));
                        app.mixer.custom_subchannel_gain[parent_ch].push(1.0);
                        app.mixer.custom_subchannel_pitch[parent_ch].push(1.0);
                        app.mixer.custom_subchannel_eq_high[parent_ch].push(1.0);
                        app.mixer.custom_subchannel_eq_mid[parent_ch].push(1.0);
                        app.mixer.custom_subchannel_eq_low[parent_ch].push(1.0);
                        app.mixer.custom_subchannel_faders[parent_ch].push(1.0);
                        app.mixer.custom_subchannel_mutes[parent_ch].push(false);
                        app.mixer.custom_subchannel_solos[parent_ch].push(false);
                        app.mixer.custom_subchannel_inserts[parent_ch].push(vec!["TRIM / GAIN".into(), "PITCH".into(), "3-BAND EQ".into()]);
                        app.mixer.custom_subchannel_insert_params[parent_ch].push(vec![[1.0; 8], [1.0; 8], [1.0; 8]]);
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new("+ SUBCHANNEL").size(7.5).strong().color(subchannel_color));
                });
            });
    });
}

#[allow(dead_code)]
pub fn render_master_strip(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    render_master_strip_full(app, ui, telemetry, false);
}

pub fn render_master_strip_full(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>, in_drawer: bool) {
    ui.push_id("master_strip", |ui| {
        let theme = app.theme;
        let accent = theme.accent;

        let sum_l = app.topo.node_map.get("master_sum_l").copied();
        let sum_r = app.topo.node_map.get("master_sum_r").copied();

        let master_peak = app.viz.damped_master_peaks[0].max(app.viz.damped_master_peaks[1]);
        let master_deck_idx = app.decks.master_deck.unwrap_or(app.decks.focused_deck);
        let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions.get(master_deck_idx % 4).copied().unwrap_or(0)).unwrap_or(0);

        Frame::none()
            .fill(theme.bg_surface)
            .rounding(Rounding::same(theme.radius_md))
            .inner_margin(Margin::same(4.0))
            .stroke(Stroke::new(1.0_f32, accent.gamma_multiply(0.6)))
            .show(ui, |ui| {
                ui.set_width(STRIP_W);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("MASTER").strong().size(theme.type_body).color(accent));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("🔍").size(8.0)).fill(theme.bg_inset)).on_hover_text("Open Master Inspector").clicked() {
                                app.mixer.focused_detail_channel = 20;
                                app.active_view = crate::View::ChannelDetail;
                            }
                        });
                    });
                    ui.add_space(2.0);

                    let is_detached = !in_drawer && app.detached_views.contains(&crate::View::Mixer);
                    if is_detached {
                        // Waveform Style Selector
                        ui.horizontal(|ui| {
                            let avail_w = ui.available_width();
                            let selected_style = app.mixer.waveform_styles[master_deck_idx % 4];
                            egui::ComboBox::from_id_source("master_wf_style")
                                .selected_text(RichText::new(selected_style.name()).size(8.0).strong().color(theme.text_primary))
                                .width(avail_w)
                                .show_ui(ui, |ui| {
                                    for st in nullherz_ui_hal::render::waveform_renderer::WaveformStyle::all() {
                                        ui.selectable_value(&mut app.mixer.waveform_styles[master_deck_idx % 4], *st, st.name());
                                    }
                                });
                        });
                        ui.add_space(2.0);

                        // Master Vertical Waveform Visualizer
                        render_vertical_waveform(app, ui, master_deck_idx % 4, elapsed_samples, master_peak, accent, &theme, telemetry);
                        ui.add_space(2.0);
                    }

                    // Master Inserts Rack
                    ui.group(|ui| {
                        ui.set_width(STRIP_W - 8.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("MASTER BUS").size(theme.type_caption).strong().color(theme.text_secondary));
                            ui.add_space(2.0);

                            // Master Gain Knob
                            let mut m_gain = app.mixer.master_gain;
                            if widgets::render_knob_sized(ui, &mut m_gain, 0.0..=2.0, "TRIM", accent, 20.0).changed() {
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

                            ui.add_space(2.0);

                            // Master EQ
                            let master_eq_node = app.topo.node_map.get("master_eq").copied();
                            let mut hi = app.mixer.mastering_eq_high;
                            if widgets::render_knob_sized(ui, &mut hi, 0.0..=2.0, "HI", accent, 18.0).changed() {
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

                            ui.add_space(1.0);

                            let mut mid = app.mixer.mastering_eq_mid;
                            if widgets::render_knob_sized(ui, &mut mid, 0.0..=2.0, "MID", accent, 18.0).changed() {
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

                            ui.add_space(1.0);

                            let mut low = app.mixer.mastering_eq_low;
                            if widgets::render_knob_sized(ui, &mut low, 0.0..=2.0, "LOW", accent, 18.0).changed() {
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

                    ui.add_space(theme.space_sm);

                    // Master Fader & Stereo VU Meters
                    ui.horizontal(|ui| {
                        let fader_w = 20.0;
                        let pad = (ui.available_width() - fader_w).max(0.0) / 2.0;
                        ui.add_space(pad);

                        let r_fader = widgets::render_fader(ui, &mut app.mixer.master_gain, 0.0..=1.2, accent, FADER_H, 22.0);
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

                        ui.add_space(2.0);

                        widgets::render_vu_meter(ui, app.viz.damped_master_peaks[0], app.mixer.master_peak_hold, accent, FADER_H);
                        ui.add_space(1.0);
                        widgets::render_vu_meter(ui, app.viz.damped_master_peaks[1], app.mixer.master_peak_hold, accent, FADER_H);
                    });

                    ui.add_space(2.0);

                    // Master Output Selector
                    ui.horizontal(|ui| {
                        let avail_w = ui.available_width();
                        let selected_output = app.mixer.master_output_source;
                        egui::ComboBox::from_id_source("master_output_src")
                            .selected_text(RichText::new(selected_output.name()).size(8.0).strong().color(theme.text_primary))
                            .width(avail_w)
                            .show_ui(ui, |ui| {
                                for out in MasterOutput::all() {
                                    ui.selectable_value(&mut app.mixer.master_output_source, *out, out.name());
                                }
                            });
                    });
                    ui.add_space(2.0);

                    // Global Play/Stop
                    ui.horizontal(|ui| {
                        let is_playing = app.decks.global_playing;
                        let play_icon = if is_playing { egui_phosphor::regular::PAUSE } else { egui_phosphor::regular::PLAY };
                        let play_btn = if is_playing {
                            egui::Button::new(RichText::new(play_icon).size(10.0).strong()).fill(accent)
                        } else {
                            egui::Button::new(RichText::new(play_icon).size(10.0).strong()).fill(theme.bg_inset)
                        };

                        if ui.add_sized([STRIP_W - 10.0, 18.0], play_btn).clicked() {
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
    });
}
