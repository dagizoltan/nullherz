use nullherz_dna::GeneticLibrary;
use egui::{Color32, Ui, Frame, Vec2, Sense, Stroke, RichText, Margin};
use crate::{InspectorApp, widgets};
use audio_core::Telemetry;
use egui_wgpu::wgpu;
use std::sync::{Arc, Mutex};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        ui.heading("Production Sampler & Capture");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.sampler_is_recording {
                let time = ui.input(|i| i.time);
                let alpha = ((time * 3.0).sin() * 0.5 + 0.5) as f32;
                ui.label(RichText::new("● RECORDING").color(Color32::RED.gamma_multiply(alpha)).strong());
            }
        });
    });
    ui.add_space(10.0);

    // Waveform Preview Area
    Frame::none().fill(Color32::from_rgb(10, 10, 12)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
        let (rect, _response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 160.0), Sense::hover());

        if let (Some(wgpu_mtx), Some(wf)) = (&app.wgpu_renderer, &app.waveform_renderer) {
             let _wgpu = wgpu_mtx.lock().unwrap();

             let deck_idx = app.focused_deck;
             if let Some(track_id) = app.now_playing[deck_idx] {
                 if let Ok(Some(track)) = app.library_db.get_track(track_id) {
                     wf.update_from_mip_waveform(&_wgpu.queue, &track.metadata.mip_waveform, app.sampler_waveform_zoom, rect.width() as u32);
                 }
             }

             if let Some(t) = telemetry {
                 let scroll = (t.beat_position as f32 % 4.0) / 4.0 * 2.0;
                 let color = app.theme.accent.to_array().map(|v| v as f32 / 255.0);
                 wf.update_globals(&_wgpu.queue, scroll, app.sampler_waveform_zoom, color);
             }

             struct WaveformCallback {
                 renderer: Arc<nullherz_ui_hal::render::waveform_renderer::WaveformRenderer>,
             }
             impl egui_wgpu::CallbackTrait for WaveformCallback {
                 fn paint<'a>(&'a self, _info: egui::PaintCallbackInfo, render_pass: &mut wgpu::RenderPass<'a>, _resources: &egui_wgpu::CallbackResources) {
                     self.renderer.render(render_pass);
                 }
             }

             let callback = egui_wgpu::Callback::new_paint_callback(rect, WaveformCallback { renderer: wf.clone() });
             ui.painter().add(callback);
        }

        if let Some(t) = telemetry {
            let playhead_x = rect.left() + (t.beat_position as f32 % 4.0) / 4.0 * rect.width();
            ui.painter().line_segment([egui::pos2(playhead_x, rect.top()), egui::pos2(playhead_x, rect.bottom())], Stroke::new(1.5, app.theme.accent));
        }
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Zoom:");
        ui.add(egui::Slider::new(&mut app.sampler_waveform_zoom, 1.0..=32.0).logarithmic(true).show_value(false));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
             if ui.button("RESET VIEW").clicked() { app.sampler_waveform_zoom = 1.0; }
        });
    });

    ui.add_space(15.0);

    ui.columns(2, |cols| {
        // Column 1: Capture Controls
        let ui = &mut cols[0];
        ui.vertical(|ui| {
            ui.heading("Capture Settings");
            ui.add_space(8.0);
            Frame::group(ui.style()).inner_margin(Margin::same(12.0)).show(ui, |ui| {
                egui::Grid::new("capture_settings_grid").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                    ui.label("Input Source");
                    if egui::ComboBox::from_id_source("input_src")
                        .selected_text(match app.sampler_input_source {
                            0 => "MASTER OUT",
                            1 => "DECK A",
                            2 => "DECK B",
                            3 => "DECK C",
                            4 => "DECK D",
                            _ => "EXTERNAL",
                        })
                        .show_ui(ui, |ui| {
                            let mut changed = false;
                            if ui.selectable_value(&mut app.sampler_input_source, 0, "MASTER OUT").clicked() { changed = true; }
                            if ui.selectable_value(&mut app.sampler_input_source, 1, "DECK A").clicked() { changed = true; }
                            if ui.selectable_value(&mut app.sampler_input_source, 2, "DECK B").clicked() { changed = true; }
                            if ui.selectable_value(&mut app.sampler_input_source, 3, "DECK C").clicked() { changed = true; }
                            if ui.selectable_value(&mut app.sampler_input_source, 4, "DECK D").clicked() { changed = true; }
                            if ui.selectable_value(&mut app.sampler_input_source, 5, "EXTERNAL IN").clicked() { changed = true; }
                            changed
                        }).inner.unwrap_or(false) {
                            // Routing Logic: Connect selected source to node 110 (Capture)
                            let src_node = match app.sampler_input_source {
                                0 => 30, // Summing/Master
                                1 => 4,  // Deck A Gain (heuristic)
                                2 => 8,  // Deck B Gain
                                3 => 12, // Deck C Gain
                                4 => 16, // Deck D Gain
                                _ => 0,  // Hardware In
                            };
                            let _ = app.command_sender.send(nullherz_traits::Command::Topology(nullherz_traits::TopologyCommand::Connect {
                                src_node_idx: src_node,
                                src_output_idx: 0,
                                dst_node_idx: 110,
                                dst_input_idx: 0,
                            }));
                        }
                    ui.end_row();

                    ui.label("Input Gain");
                    ui.horizontal(|ui| {
                        if ui.add(egui::Slider::new(&mut app.sampler_input_gain, 0.0..=4.0)).changed() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: 110, param_id: 0, value: app.sampler_input_gain, ramp_duration_samples: 0,
                            }));
                        }
                        if let Some(t) = telemetry {
                            let level = t.peak_levels.get(app.sampler_input_source).cloned().unwrap_or(0.0);
                            widgets::render_vu_meter(ui, level, app.channel_peak_hold[0], app.theme.accent, 20.0);
                        }
                    });
                    ui.end_row();

                    ui.label("Monitor");
                    if ui.add(egui::Slider::new(&mut app.sampler_monitor_level, 0.0..=1.0)).changed() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                            target_id: 110, param_id: 1, value: app.sampler_monitor_level, ramp_duration_samples: 0,
                        }));
                    }
                    ui.end_row();

                    ui.label("Config");
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut app.sampler_is_stereo, "Stereo").changed() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: 110, param_id: 2, value: if app.sampler_is_stereo { 1.0 } else { 0.0 }, ramp_duration_samples: 0,
                            }));
                        }
                    });
                    ui.end_row();
                });

                ui.add_space(15.0);
                ui.horizontal(|ui| {
                    let rec_btn = if app.sampler_is_recording {
                        egui::Button::new(RichText::new("■ STOP").strong().color(Color32::WHITE)).fill(Color32::from_rgb(180, 0, 0))
                    } else {
                        egui::Button::new(RichText::new("● RECORD").strong().color(Color32::RED))
                    };

                    if ui.add(rec_btn.min_size(Vec2::new(100.0, 32.0))).clicked() {
                        app.sampler_is_recording = !app.sampler_is_recording;
                        let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                            target_id: 110, param_id: 3, value: if app.sampler_is_recording { 1.0 } else { 0.0 }, ramp_duration_samples: 0,
                        }));
                    }

                    if ui.add(egui::Button::new("RESET").min_size(Vec2::new(60.0, 32.0))).clicked() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                            target_id: 110, param_id: 4, value: 1.0, ramp_duration_samples: 0,
                        }));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("COMMIT").strong().color(app.theme.accent)).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::RegisterCapture {
                                capture_node_idx: 110, sample_id: 0,
                            }));
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
            Frame::group(ui.style()).inner_margin(Margin::same(12.0)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut app.sampler_slicer_mode, "ENABLE SLICER").changed() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                            target_id: 100, param_id: 3, value: if app.sampler_slicer_mode { 1.0 } else { 0.0 }, ramp_duration_samples: 0,
                        }));
                    }
                });

                ui.add_space(10.0);
                ui.label(RichText::new("PERFORMANCE PADS").small().color(Color32::from_gray(100)));
                egui::Grid::new("slicer_pads").spacing([8.0, 8.0]).show(ui, |ui| {
                    for row in 0..2 {
                        for col in 0..8 {
                            let idx = row * 8 + col;
                            let is_active = telemetry.as_ref().map(|t| (t.beat_position as usize % 16) == idx).unwrap_or(false);

                            let btn = egui::Button::new(RichText::new(format!("{}", idx + 1)).strong())
                                .min_size(Vec2::splat(36.0))
                                .fill(if is_active { Color32::from_rgb(0, 150, 120) } else { Color32::from_gray(40) });

                            if ui.add(btn).clicked() {
                                let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::TriggerSlice {
                                    node_idx: 100, slice_idx: idx as u32,
                                }));
                            }
                        }
                        ui.end_row();
                    }
                });
            });
        });
    });
}
