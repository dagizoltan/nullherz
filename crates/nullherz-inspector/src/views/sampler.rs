use nullherz_dna::GeneticLibrary;
use egui::{Ui, Frame, Vec2, Sense, Stroke, RichText};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        ui.heading("Production Sampler & Capture");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.sampler.sampler_is_recording {
                let time = ui.input(|i| i.time);
                let alpha = ((time * 3.0).sin() * 0.5 + 0.5) as f32;
                ui.label(RichText::new("● RECORDING").color(app.theme.danger.gamma_multiply(alpha)).strong());
            }
        });
    });
    ui.add_space(10.0);

    // Waveform Preview Area - Decoupled to Theme tokens
    Frame::none()
        .fill(app.theme.bg_dark)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_md)
        .show(ui, |ui| {
            let (rect, _response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 160.0), Sense::hover());

            if let (Some(wgpu_mtx), Some(wf_mtx)) = (&app.wgpu_renderer, &app.waveform_renderer) {
                 let _wgpu = wgpu_mtx.lock();
                 let mut wf = wf_mtx.lock();

                 let deck_idx = app.decks.focused_deck;
                 if let Some(track_id) = app.decks.now_playing[deck_idx]
                     && let Ok(Some(track)) = app.library_db.get_track(track_id) {
                         wf.update_from_mip_waveform(&_wgpu.queue, &track.metadata.mip_waveform, app.sampler.sampler_waveform_zoom, rect.width() as u32);
                     }

                 if let Some(t) = telemetry {
                     let scroll = (t.get_interpolated_beat_position() as f32 % 4.0) / 4.0 * 2.0;
                     let color = app.theme.accent.to_array().map(|v| v as f32 / 255.0);
                     wf.update_globals(&_wgpu.queue, scroll, app.sampler.sampler_waveform_zoom, color);
                 }

                 nullherz_ui_hal::render::waveform_renderer::ui_paint_waveform(ui, rect, wf_mtx.clone());
            }

            if let Some(t) = telemetry {
                let playhead_x = rect.left() + (t.get_interpolated_beat_position() as f32 % 4.0) / 4.0 * rect.width();
                ui.painter().line_segment([egui::pos2(playhead_x, rect.top()), egui::pos2(playhead_x, rect.bottom())], Stroke::new(1.5, app.theme.accent));
            }
        });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Zoom:");
        ui.add(egui::Slider::new(&mut app.sampler.sampler_waveform_zoom, 1.0..=32.0).logarithmic(true).show_value(false));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
             if ui.button("RESET VIEW").clicked() { app.sampler.sampler_waveform_zoom = 1.0; }
        });
    });

    ui.add_space(15.0);

    ui.columns(2, |cols| {
        // Column 1: Capture Controls
        let ui = &mut cols[0];
        ui.vertical(|ui| {
            ui.heading("Capture Settings");
            ui.add_space(8.0);
            Frame::none()
                .fill(app.theme.bg_surface)
                .rounding(app.theme.radius_md)
                .stroke(app.theme.border_stroke)
                .inner_margin(app.theme.space_md)
                .show(ui, |ui| {
                    egui::Grid::new("capture_settings_grid").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                        ui.label("Input Source");
                        let options = [
                            (0, "MST"),
                            (1, "A"),
                            (2, "B"),
                            (3, "C"),
                            (4, "D"),
                            (5, "EXT"),
                        ];
                        let old_source = app.sampler.sampler_input_source;
                        nullherz_ui_hal::widgets::render_segmented_control(
                            ui,
                            &app.theme,
                            &mut app.sampler.sampler_input_source,
                            &options,
                        );
                        if app.sampler.sampler_input_source != old_source {
                            // Routing Logic: Connect selected source to Capture node
                            let src_node = match app.sampler.sampler_input_source {
                                0 => app.get_node_id("master_sum_l"),
                                1 => app.get_node_id("deck_a_gain"),
                                2 => app.get_node_id("deck_b_gain"),
                                3 => app.get_node_id("deck_c_gain"),
                                4 => app.get_node_id("deck_d_gain"),
                                _ => None, // Hardware In: no graph source
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
                        ui.end_row();

                        ui.label("Input Gain");
                        ui.horizontal(|ui| {
                            if ui.add(egui::Slider::new(&mut app.sampler.sampler_input_gain, 0.0..=4.0)).changed() {
                                if let Some(resolved_node) = app.get_node_id("capture_node") {
                                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                        target_id: resolved_node as u64, param_id: 0, value: app.sampler.sampler_input_gain, ramp_duration_samples: 0,
                                    }));
                                }
                            }
                            if let Some(t) = telemetry {
                                let level = t.peak_levels.get(app.sampler.sampler_input_source).cloned().unwrap_or(0.0);
                                widgets::render_vu_meter(ui, level, app.mixer.channel_peak_hold[0], app.theme.accent, 20.0);
                            }
                        });
                        ui.end_row();

                        ui.label("Monitor");
                        if ui.add(egui::Slider::new(&mut app.sampler.sampler_monitor_level, 0.0..=1.0)).changed() {
                            if let Some(resolved_node) = app.get_node_id("capture_node") {
                                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: resolved_node as u64, param_id: 1, value: app.sampler.sampler_monitor_level, ramp_duration_samples: 0,
                                }));
                            }
                        }
                        ui.end_row();

                        ui.label("Config");
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut app.sampler.sampler_is_stereo, "Stereo").changed() {
                                if let Some(resolved_node) = app.get_node_id("capture_node") {
                                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                        target_id: resolved_node as u64, param_id: 2, value: if app.sampler.sampler_is_stereo { 1.0 } else { 0.0 }, ramp_duration_samples: 0,
                                    }));
                                }
                            }
                        });
                        ui.end_row();
                    });

                    ui.add_space(15.0);
                    ui.horizontal(|ui| {
                        let rec_btn = if app.sampler.sampler_is_recording {
                            egui::Button::new(RichText::new("■ STOP").strong().color(app.theme.text_primary)).fill(app.theme.danger)
                        } else {
                            egui::Button::new(RichText::new("● RECORD").strong().color(app.theme.text_primary)).fill(app.theme.danger)
                        };

                        if ui.add(rec_btn.min_size(Vec2::new(100.0, 32.0))).clicked() {
                            app.sampler.sampler_is_recording = !app.sampler.sampler_is_recording;
                            if let Some(resolved_node) = app.get_node_id("capture_node") {
                                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: resolved_node as u64, param_id: 3, value: if app.sampler.sampler_is_recording { 1.0 } else { 0.0 }, ramp_duration_samples: 0,
                                }));
                            }
                        }

                        if ui.add(egui::Button::new("RESET").min_size(Vec2::new(60.0, 32.0))).clicked() {
                            if let Some(resolved_node) = app.get_node_id("capture_node") {
                                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: resolved_node as u64, param_id: 4, value: 1.0, ramp_duration_samples: 0,
                                }));
                            }
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(RichText::new("COMMIT").strong().color(app.theme.accent)).clicked() {
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
        });

        // Column 2: Performance Slicer
        let ui = &mut cols[1];
        ui.vertical(|ui| {
            ui.heading("Loop Slicer");
            ui.add_space(8.0);
            Frame::none()
                .fill(app.theme.bg_surface)
                .rounding(app.theme.radius_md)
                .stroke(app.theme.border_stroke)
                .inner_margin(app.theme.space_md)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut app.sampler.sampler_slicer_mode, "ENABLE SLICER").changed() {
                            if let Some(resolved_node) = app.get_node_id("sampler_node") {
                                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: resolved_node as u64, param_id: 3, value: if app.sampler.sampler_slicer_mode { 1.0 } else { 0.0 }, ramp_duration_samples: 0,
                                }));
                            }
                        }
                    });

                    ui.add_space(10.0);
                    ui.label(RichText::new("PERFORMANCE PADS").small().color(app.theme.text_secondary));
                    egui::Grid::new("slicer_pads").spacing([8.0, 8.0]).show(ui, |ui| {
                        for row in 0..2 {
                            for col in 0..8 {
                                let idx = row * 8 + col;
                                let is_active = telemetry.as_ref().map(|t| (t.get_interpolated_beat_position() as usize % 16) == idx).unwrap_or(false);

                                let btn = egui::Button::new(RichText::new(format!("{}", idx + 1)).strong())
                                    .min_size(Vec2::splat(36.0))
                                    .fill(if is_active { app.theme.accent.linear_multiply(0.4) } else { app.theme.bg_inset });

                                if ui.add(btn).clicked() {
                                    if let Some(resolved_node) = app.get_node_id("sampler_node") {
                                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::TriggerSlice {
                                            node_idx: resolved_node, slice_idx: idx as u32,
                                        }));
                                    }
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
        });
    });
}
