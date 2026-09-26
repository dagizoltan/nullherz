use egui::{Ui, Frame, Margin, Rounding, Stroke, RichText, ScrollArea};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;

/// Fixed strip width: every card is the same size regardless of window
/// width. (The old layout used `vertical_centered` inside the horizontal
/// row, which expands to the FULL remaining width — the first card
/// ballooned and pushed the rest off the right edge.)
const STRIP_W: f32 = 140.0;
const FADER_H: f32 = 150.0;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    ui.heading(RichText::new("System Mixer").size(theme.type_heading));
    ui.add_space(theme.space_md);

    // Horizontal scroll instead of silent overflow on narrow windows.
    ScrollArea::horizontal().id_source("sys_mixer_scroll").show(ui, |ui| {
        ui.horizontal_top(|ui| {
            for i in 0..4 {
                render_channel_strip(app, ui, i, telemetry);
                ui.add_space(theme.space_sm);
            }
            ui.add_space(theme.space_md);
            render_master_strip(app, ui, telemetry);
        });
    });
}

fn render_channel_strip(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let deck_color = crate::InspectorApp::deck_color(&theme, i);
    let deck = ['a', 'b', 'c', 'd'][i];

    // Resolve this channel's REAL node ids from the telemetry node map.
    let gain_node = app.topo.node_map.get(&format!("deck_{}_gain", deck)).copied();
    let iso_node = app.topo.node_map.get(&format!("deck_{}_isolator", deck)).copied();
    let meter_node = iso_node.or_else(|| app.topo.node_map.get(&format!("deck_{}_sampler", deck)).copied());

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .inner_margin(Margin::same(theme.space_md))
        .stroke(Stroke::new(1.0_f32, theme.border))
        .show(ui, |ui| {
            ui.set_width(STRIP_W);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 40.0).max(0.0) / 2.0);
                    ui.label(RichText::new(format!("CH {}", (b'A' + i as u8) as char)).strong().size(theme.type_body).color(deck_color));
                });
                ui.add_space(theme.space_xs);

                // --- INSERTS RACK ---
                ui.group(|ui| {
                    ui.set_width(STRIP_W - 12.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("INSERTS RACK").size(theme.type_caption).strong().color(theme.text_secondary));
                        ui.add_space(2.0);

                        // Insert Slot 1: 3-Band EQ (HI, MID, LOW)
                        Frame::none()
                            .fill(theme.bg_inset)
                            .rounding(Rounding::same(theme.radius_sm))
                            .inner_margin(Margin::same(4.0))
                            .stroke(Stroke::new(1.0, theme.border_stroke.color))
                            .show(ui, |ui| {
                                ui.set_width(STRIP_W - 20.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new("1: 3-BAND EQ").size(9.0).strong().color(theme.accent));
                                    ui.add_space(2.0);
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 2.0;

                                        // HI Knob
                                        let mut hi = app.mixer.channel_eq_high[i];
                                        if widgets::render_knob_sized(ui, &mut hi, 0.0..=2.0, "HI", deck_color, 26.0).changed() {
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
                            });

                        ui.add_space(4.0);

                        // Insert Slot 2: Gain / Trim
                        Frame::none()
                            .fill(theme.bg_inset)
                            .rounding(Rounding::same(theme.radius_sm))
                            .inner_margin(Margin::same(4.0))
                            .stroke(Stroke::new(1.0, theme.border_stroke.color))
                            .show(ui, |ui| {
                                ui.set_width(STRIP_W - 20.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("2: GAIN").size(9.0).strong().color(theme.success));
                                    ui.add_space(4.0);
                                    let mut gain_val = app.mixer.channel_faders[i];
                                    if widgets::render_knob_sized(ui, &mut gain_val, 0.0..=2.0, "", deck_color, 24.0).changed() {
                                        app.mixer.channel_faders[i] = gain_val;
                                        if let Some(gain_id) = gain_node {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: gain_id as u64,
                                                param_id: 0,
                                                value: gain_val,
                                                ramp_duration_samples: 128,
                                            }));
                                        }
                                    }
                                });
                            });

                        ui.add_space(4.0);

                        // Attached FX and + FX button
                        let mut remove_insert = false;
                        if let Some(ref name) = app.decks.deck_inserts[i] {
                            let name_clone = name.clone();
                            Frame::none()
                                .fill(theme.accent.linear_multiply(0.2))
                                .rounding(Rounding::same(theme.radius_sm))
                                .inner_margin(Margin::same(4.0))
                                .stroke(Stroke::new(1.0, theme.accent))
                                .show(ui, |ui| {
                                    ui.set_width(STRIP_W - 20.0);
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(format!("FX: {}", name_clone)).size(9.0).strong().color(theme.text_primary));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.button(RichText::new("×").size(10.0).strong()).clicked() {
                                                remove_insert = true;
                                            }
                                        });
                                    });
                                });
                            ui.add_space(2.0);
                        }
                        if remove_insert {
                            app.decks.deck_inserts[i] = None;
                        }

                        if ui.add_sized([STRIP_W - 20.0, 18.0], egui::Button::new(RichText::new("+ FX").size(9.0).strong()).fill(theme.bg_inset)).clicked() {
                            app.active_right_tab = Some(crate::RightTab::Store);
                            app.store.active_tag_filter = Some("insert".to_string());
                        }
                    });
                });

                ui.add_space(theme.space_md);

                // --- VOLUME FADER & STEREO VU METERS ---
                ui.horizontal(|ui| {
                    let r_fader = widgets::render_fader(ui, &mut app.mixer.channel_faders[i], 0.0..=1.2, deck_color, FADER_H, 30.0);
                    if r_fader.changed()
                        && let Some(gain_id) = gain_node {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: gain_id as u64,
                                param_id: 0,
                                value: app.mixer.channel_faders[i],
                                ramp_duration_samples: 128,
                            }));
                        }
                    if r_fader.drag_stopped() || r_fader.lost_focus() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CheckpointParameterEdit));
                    }

                    ui.add_space(6.0);

                    // STEREO VU METERS (Left & Right channels side-by-side)
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
                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 50.0).max(0.0) / 2.0);
                    ui.label(
                        RichText::new(format!("{:+.1} dB", 20.0 * app.mixer.channel_faders[i].max(1e-3).log10()))
                            .monospace()
                            .size(theme.type_caption)
                            .color(theme.text_secondary),
                    );
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // --- TRACK INFO ---
                if let Some(ref track) = app.decks.cached_tracks[i] {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(&track.title).size(10.0).strong().color(theme.text_primary));
                        if !track.artist.is_empty() {
                            ui.label(RichText::new(&track.artist).size(9.0).color(theme.text_secondary));
                        }
                    });
                } else {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("No Track Loaded").size(9.0).italics().color(theme.text_disabled));
                    });
                }
            });
        });
}

fn render_master_strip(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let accent = theme.accent;

    // Master level is applied per side: the SUMMING nodes' gain (param 0).
    let sum_l = app.topo.node_map.get("master_sum_l").copied();
    let sum_r = app.topo.node_map.get("master_sum_r").copied();

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

                // --- MASTER INSERTS RACK ---
                ui.group(|ui| {
                    ui.set_width(STRIP_W - 12.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("MASTER INSERTS").size(theme.type_caption).strong().color(theme.text_secondary));
                        ui.add_space(2.0);

                        ui.add_sized([STRIP_W - 20.0, 18.0], egui::Button::new(RichText::new("+ FX").size(9.0).strong()).fill(theme.bg_inset)).clicked().then(|| {
                            app.active_right_tab = Some(crate::RightTab::Store);
                            app.store.active_tag_filter = Some("insert".to_string());
                        });
                    });
                });

                ui.add_space(theme.space_sm);

                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 24.0 - 20.0 - theme.space_sm).max(0.0) / 2.0);
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

                    ui.add_space(theme.space_sm);
                    // Stereo pair: damped master peaks are bound to
                    // master_sum_l/r in the update loop.
                    let _ = telemetry;
                    widgets::render_vu_meter(ui, app.viz.damped_master_peaks[0], app.mixer.master_peak_hold, accent, FADER_H);
                    ui.add_space(2.0);
                    widgets::render_vu_meter(ui, app.viz.damped_master_peaks[1], app.mixer.master_peak_hold, accent, FADER_H);
                });

                ui.add_space(theme.space_xs);
                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 34.0).max(0.0) / 2.0);
                    ui.label(
                        RichText::new(format!("{:+.1} dB", 20.0 * app.mixer.master_gain.max(1e-3).log10()))
                            .monospace()
                            .size(theme.type_caption)
                            .color(theme.text_secondary),
                    );
                });
            });
        });
}
