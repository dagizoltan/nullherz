use egui::{Ui, Frame, Vec2, Sense, RichText, Stroke, Margin, Color32};
use nullherz_ui_hal::widgets::{self, render_knob_sized};
use audio_core::Telemetry;
use nullherz_traits::{Command, CoreCommand, MidiEvent, MixerCommand};
use crate::InspectorApp;

pub fn render_instrument_drawer(app: &mut InspectorApp, ui: &mut Ui, _telemetry: &Option<Telemetry>) {
    Frame::none()
        .fill(app.theme.bg_surface)
        .rounding(app.theme.radius_md)
        .inner_margin(app.theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("PRODUCTION STUDIO SAMPLER & CAPTURE ENGINE").strong().size(app.theme.type_body));
            });
            ui.add_space(4.0);
            render(app, ui, _telemetry);
        });
}

pub fn render(app: &mut InspectorApp, ui: &mut Ui, _telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let current_time = ui.input(|i| i.time);

    // --- Header & Recording Status Banner ---
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(Stroke::new(1.0, theme.border))
        .inner_margin(Margin::symmetric(theme.space_md, theme.space_xs))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(egui_phosphor::regular::MICROPHONE_STAGE).size(16.0).color(theme.accent));
                ui.add_space(4.0);
                ui.label(RichText::new("STUDIO SAMPLER & ZERO-LATENCY CAPTURE").strong().size(theme.type_body).color(theme.text_primary));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if app.sampler.sampler_is_recording {
                        let alpha = ((current_time * 4.0).sin() * 0.5 + 0.5) as f32;
                        Frame::none()
                            .fill(theme.danger.linear_multiply(0.2))
                            .rounding(theme.radius_sm)
                            .stroke(Stroke::new(1.0, theme.danger))
                            .inner_margin(Margin::symmetric(8.0, 3.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new("● RECORDING IN PROGRESS").size(10.0).strong().color(theme.danger.gamma_multiply(alpha)));
                            });
                    } else {
                        Frame::none()
                            .fill(theme.bg_inset)
                            .rounding(theme.radius_sm)
                            .stroke(Stroke::new(1.0, theme.border_stroke.color))
                            .inner_margin(Margin::symmetric(8.0, 3.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new("READY TO CAPTURE").size(10.0).strong().color(theme.success));
                            });
                    }
                });
            });
        });

    ui.add_space(theme.space_sm);

    // --- Primary Waveform Preview & Replay Canvas ---
    Frame::none()
        .fill(theme.bg_dark)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(Margin::same(theme.space_md))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} CAPTURED SAMPLE BUFFER", egui_phosphor::regular::WAVEFORM)).strong().size(theme.type_caption).color(theme.accent));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new(format!("{} RESET ZOOM", egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)).size(theme.type_caption)).clicked() {
                            app.sampler.sampler_waveform_zoom = 1.0;
                        }
                        ui.add_space(theme.space_xs);
                        ui.add(egui::Slider::new(&mut app.sampler.sampler_waveform_zoom, 1.0..=32.0).logarithmic(true).show_value(false));
                        ui.label(RichText::new("ZOOM:").size(theme.type_caption).strong());
                    });
                });

                ui.add_space(theme.space_xs);

                // Waveform Painter
                let (rect, _response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 120.0), Sense::hover());
                ui.painter().rect_filled(rect, theme.radius_sm, theme.bg_dark);
                ui.painter().rect_stroke(rect, theme.radius_sm, Stroke::new(1.0, theme.border_stroke.color));

                if let (Some(wgpu_mtx), Some(wf_mtx)) = (&app.wgpu_renderer, &app.waveform_renderer) {
                    let wgpu = wgpu_mtx.lock();
                    let mut wf = wf_mtx.lock();

                    if let Some(track_id) = app.sampler.source_track.or(app.library.selected_library_track) {
                        if let Some(track) = app.get_cached_track(track_id) {
                            wf.update_from_mip_waveform(&wgpu.queue, &track.metadata.mip_waveform, app.sampler.sampler_waveform_zoom, rect.width() as u32, theme.accent.to_array().map(|v| v as f32 / 255.0));
                        }
                    }

                    let color = theme.accent.to_array().map(|v| v as f32 / 255.0);
                    wf.update_globals(&wgpu.queue, 0.0, app.sampler.sampler_waveform_zoom, false, app.mixer.waveform_styles[0], color);

                    nullherz_ui_hal::render::waveform_renderer::ui_paint_waveform(ui, rect, wf_mtx.clone());
                }
            });
        });

    ui.add_space(theme.space_sm);

    // --- Two-Column Layout: Input Selection & Transport + FX Inserts Rack ---
    ui.columns(2, |cols| {
        // Left Column: Input Selection, Transport & Recording Controls
        let ui = &mut cols[0];
        Frame::none()
            .fill(theme.bg_surface)
            .rounding(theme.radius_md)
            .stroke(Stroke::new(1.0, theme.border))
            .inner_margin(Margin::same(theme.space_md))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("{} INPUT ROUTING & RECORDING", egui_phosphor::regular::SLIDERS)).strong().size(theme.type_body).color(theme.accent));
                    ui.add_space(theme.space_xs);

                    // Segmented Input Selector
                    ui.label(RichText::new("Input Source:").size(theme.type_caption).strong());
                    let options = [
                        (0, "MST"),
                        (1, "Deck A"),
                        (2, "Deck B"),
                        (3, "Deck C"),
                        (4, "Deck D"),
                        (5, "EXT In"),
                    ];
                    let old_source = app.sampler.sampler_input_source;
                    widgets::render_segmented_control(
                        ui,
                        &theme,
                        &mut app.sampler.sampler_input_source,
                        &options,
                    );

                    if app.sampler.sampler_input_source != old_source {
                        let src_node = match app.sampler.sampler_input_source {
                            0 => app.get_node_id("master_sum_l"),
                            1 => app.get_node_id("deck_a_gain"),
                            2 => app.get_node_id("deck_b_gain"),
                            3 => app.get_node_id("deck_c_gain"),
                            4 => app.get_node_id("deck_d_gain"),
                            _ => None,
                        };
                        if let (Some(src_node), Some(capture_node)) = (src_node, app.get_node_id("capture_node")) {
                            let _ = app.command_sender.send(nullherz_traits::Command::Topology(nullherz_traits::TopologyCommand::Connect {
                                src_node_idx: src_node,
                                src_output_idx: 0,
                                dst_node_idx: capture_node,
                                dst_input_idx: 0,
                            }));
                        }
                    }

                    ui.add_space(theme.space_sm);

                    // Gain & Monitor Rotary Knobs with VU Meter
                    Frame::none()
                        .fill(theme.bg_inset)
                        .rounding(theme.radius_sm)
                        .stroke(Stroke::new(1.0, theme.border_stroke.color))
                        .inner_margin(Margin::same(theme.space_sm))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 20.0;

                                // Gain Knob
                                ui.horizontal(|ui| {
                                    if render_knob_sized(ui, &mut app.sampler.sampler_input_gain, 0.0..=4.0, "GAIN", theme.accent, 24.0).changed() {
                                        if let Some(resolved_node) = app.get_node_id("capture_node") {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: resolved_node as u64, param_id: 0, value: app.sampler.sampler_input_gain, ramp_duration_samples: 0,
                                            }));
                                        }
                                    }
                                    ui.add_space(2.0);
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new("INPUT GAIN").strong().size(9.0).color(theme.accent));
                                        ui.label(RichText::new(format!("{:.1}x", app.sampler.sampler_input_gain)).size(10.0).color(theme.text_primary));
                                    });
                                });

                                // Monitor Knob
                                ui.horizontal(|ui| {
                                    if render_knob_sized(ui, &mut app.sampler.sampler_monitor_level, 0.0..=1.0, "MON", theme.warning, 24.0).changed() {
                                        if let Some(resolved_node) = app.get_node_id("capture_node") {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: resolved_node as u64, param_id: 1, value: app.sampler.sampler_monitor_level, ramp_duration_samples: 0,
                                            }));
                                        }
                                    }
                                    ui.add_space(2.0);
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new("MONITOR").strong().size(9.0).color(theme.warning));
                                        ui.label(RichText::new(format!("{:.0}%", app.sampler.sampler_monitor_level * 100.0)).size(10.0).color(theme.text_primary));
                                    });
                                });

                                // Format Toggle
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut app.sampler.sampler_is_stereo, "Stereo").changed() {
                                        if let Some(resolved_node) = app.get_node_id("capture_node") {
                                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                                target_id: resolved_node as u64, param_id: 2, value: if app.sampler.sampler_is_stereo { 1.0 } else { 0.0 }, ramp_duration_samples: 0,
                                            }));
                                        }
                                    }
                                });
                            });
                        });

                    ui.add_space(theme.space_sm);

                    // Transport & Capture Buttons Row
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;

                        // RECORD / STOP Toggle
                        let rec_icon = if app.sampler.sampler_is_recording { egui_phosphor::regular::SQUARE } else { egui_phosphor::regular::RECORD };
                        let rec_label = if app.sampler.sampler_is_recording { "STOP" } else { "RECORD" };
                        let rec_bg = if app.sampler.sampler_is_recording { theme.danger } else { theme.accent };

                        let rec_btn = egui::Button::new(RichText::new(format!("{} {}", rec_icon, rec_label)).strong().size(theme.type_body).color(Color32::BLACK)).fill(rec_bg);

                        if ui.add_sized([120.0, 32.0], rec_btn).clicked() {
                            app.sampler.sampler_is_recording = !app.sampler.sampler_is_recording;
                            if let Some(resolved_node) = app.get_node_id("capture_node") {
                                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: resolved_node as u64, param_id: 3, value: if app.sampler.sampler_is_recording { 1.0 } else { 0.0 }, ramp_duration_samples: 0,
                                }));
                            }
                        }

                        // REPLAY RECORDED Button
                        let replay_btn = egui::Button::new(RichText::new(format!("{} REPLAY", egui_phosphor::regular::PLAY)).strong().size(theme.type_body).color(theme.text_primary)).fill(theme.bg_inset);
                        if ui.add_sized([110.0, 32.0], replay_btn).on_hover_text("Play recorded sample buffer").clicked() {
                            let event = MidiEvent {
                                timestamp_samples: 0,
                                status: 0x90,
                                data1: 60,
                                data2: 127,
                                _pad: 0,
                            };
                            let _ = app.command_sender.send(Command::Core(CoreCommand::InjectMidi(event)));
                        }

                        // COMMIT TO LIBRARY
                        let commit_btn = egui::Button::new(RichText::new(format!("{} SAVE", egui_phosphor::regular::FLOPPY_DISK)).strong().size(theme.type_body).color(theme.text_primary)).fill(theme.bg_inset);
                        if ui.add_sized([90.0, 32.0], commit_btn).on_hover_text("Save sample into library assets").clicked() {
                            let sample_id = app.sampler.next_sample_id;
                            app.sampler.next_sample_id += 1;
                            if let Some(resolved_node) = app.get_node_id("capture_node") {
                                let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::RegisterCapture {
                                    capture_node_idx: resolved_node, sample_id,
                                }));
                            }
                        }
                    });
                });
            });

        // Right Column: FX Inserts Rack
        let ui = &mut cols[1];
        Frame::none()
            .fill(theme.bg_surface)
            .rounding(theme.radius_md)
            .stroke(Stroke::new(1.0, theme.border))
            .inner_margin(Margin::same(theme.space_md))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{} MODULAR FX INSERTS RACK", egui_phosphor::regular::FADERS)).strong().size(theme.type_body).color(theme.accent));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("+ FX").size(theme.type_caption).strong()).fill(theme.bg_inset)).clicked() {
                                app.active_right_tab = Some(crate::RightTab::Store);
                                app.store.active_category = Some(sidecar_sdk::AssetCategory::AudioInsert);
                            }
                        });
                    });

                    ui.add_space(theme.space_xs);

                    let sub_idx = 0; // Main Sampler FX channel
                    let inserts = app.sampler.subchannel_inserts[sub_idx].clone();
                    let mut fx_to_remove = None;

                    if inserts.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(20.0);
                            ui.label(RichText::new("NO FX INSERTS ATTACHED").size(theme.type_caption).color(theme.text_secondary));
                            ui.label(RichText::new("Click '+ FX' to load audio processors from Store inventory.").size(8.5).color(theme.text_secondary));
                            ui.add_space(20.0);
                        });
                    } else {
                        for (fx_i, fx_name) in inserts.iter().enumerate() {
                            Frame::none()
                                .fill(theme.bg_inset)
                                .rounding(theme.radius_sm)
                                .stroke(Stroke::new(1.0, theme.border_stroke.color))
                                .inner_margin(Margin::same(theme.space_xs))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(format!("• {}", fx_name)).strong().size(theme.type_caption).color(theme.accent));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.button(RichText::new("×").size(10.0).strong().color(theme.danger)).clicked() {
                                                fx_to_remove = Some(fx_i);
                                            }
                                        });
                                    });

                                    ui.add_space(2.0);

                                    // Dynamic Insert Controls
                                    if fx_name == "3-BAND EQ" {
                                        ui.horizontal(|ui| {
                                            render_knob_sized(ui, &mut app.sampler.subchannel_eq_high[sub_idx], 0.0..=2.0, "HI", theme.accent, 20.0);
                                            render_knob_sized(ui, &mut app.sampler.subchannel_eq_mid[sub_idx], 0.0..=2.0, "MID", theme.accent, 20.0);
                                            render_knob_sized(ui, &mut app.sampler.subchannel_eq_low[sub_idx], 0.0..=2.0, "LOW", theme.accent, 20.0);
                                        });
                                    } else if fx_name == "PITCH" {
                                        let mut tune = app.sampler.pad_tune[sub_idx];
                                        if render_knob_sized(ui, &mut tune, 0.0..=1.0, "PITCH", theme.warning, 20.0).changed() {
                                            app.sampler.pad_tune[sub_idx] = tune;
                                            if let Some(dm_node) = app.get_node_id("capture_node") {
                                                let _ = app.command_sender.send(Command::Mixer(MixerCommand::SetParam {
                                                    target_id: dm_node as u64, param_id: 5, value: tune, ramp_duration_samples: 0,
                                                }));
                                            }
                                        }
                                    } else {
                                        let mut p_val = app.sampler.subchannel_insert_params[sub_idx].get(fx_i).map(|p| p[0]).unwrap_or(0.5);
                                        if render_knob_sized(ui, &mut p_val, 0.0..=1.0, "MIX", theme.accent, 20.0).changed() {
                                            if let Some(p) = app.sampler.subchannel_insert_params[sub_idx].get_mut(fx_i) { p[0] = p_val; }
                                        }
                                    }
                                });
                            ui.add_space(4.0);
                        }

                        if let Some(rem_i) = fx_to_remove {
                            if rem_i < app.sampler.subchannel_inserts[sub_idx].len() {
                                app.sampler.subchannel_inserts[sub_idx].remove(rem_i);
                                app.sampler.subchannel_insert_params[sub_idx].remove(rem_i);
                            }
                        }
                    }
                });
            });
    });
}
