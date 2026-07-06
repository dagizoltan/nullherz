use nullherz_dna::GeneticLibrary;
use egui::{Color32, Ui, Frame, Vec2, Sense, Stroke, RichText};
use crate::InspectorApp;
use audio_core::Telemetry;
use egui_wgpu::wgpu;
use std::sync::{Arc, Mutex};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        ui.heading("Production Sampler");
    });
    ui.add_space(10.0);

    Frame::none().fill(Color32::from_rgb(10, 10, 12)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
        let (rect, _response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 200.0), Sense::hover());

        // WGPU Accelerated Waveform rendering callback
        if let (Some(wgpu_mtx), Some(wf_mtx)) = (&app.wgpu_renderer, &app.waveform_renderer) {
             let _wgpu = wgpu_mtx.lock().unwrap();
             let mut wf = wf_mtx.lock().unwrap();

             // Use actual track data if available
             let deck_idx = app.focused_deck;
             if let Some(track_id) = app.now_playing[deck_idx] {
                 if let Ok(Some(track)) = app.library_db.get_track(track_id) {
                     wf.update_from_mip_waveform(&_wgpu.queue, &track.metadata.mip_waveform, app.sampler_waveform_zoom, rect.width() as u32);
                 }
             }

             if let Some(t) = telemetry {
                 // bit-exact scrolling based on beat position
                 let scroll = (t.beat_position as f32 % 4.0) / 4.0 * 2.0;
                 wf.update_globals(&_wgpu.queue, scroll, app.sampler_waveform_zoom, [0.0, 1.0, 0.8, 1.0]);
             }

             // Setup callback for WGPU rendering
             struct WaveformCallback {
                 renderer: Arc<Mutex<nullherz_ui_hal::render::waveform_renderer::WaveformRenderer>>,
             }
             impl egui_wgpu::CallbackTrait for WaveformCallback {
                 fn paint<'a>(&'a self, _info: egui::PaintCallbackInfo, render_pass: &mut wgpu::RenderPass<'a>, _resources: &egui_wgpu::CallbackResources) {
                     if let Ok(wf) = self.renderer.lock() {
                         let wf_ptr: *const nullherz_ui_hal::render::waveform_renderer::WaveformRenderer = &*wf;
                         unsafe {
                             (*wf_ptr).render(render_pass);
                         }
                     }
                 }
             }

             let callback = egui_wgpu::Callback::new_paint_callback(rect, WaveformCallback { renderer: wf_mtx.clone() });
             ui.painter().add(callback);

             ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "GPU-ACCELERATED WAVEFORM ENGINE ACTIVE", egui::FontId::proportional(14.0), Color32::from_rgb(0, 100, 80));
        } else {
             ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "WGPU Accelerated Waveform (120fps)", egui::FontId::proportional(14.0), Color32::GRAY);
        }

        if let Some(t) = telemetry {
            // Visualize real-time playhead bit-exactly
            let playhead_x = rect.left() + (t.beat_position as f32 % 4.0) / 4.0 * rect.width();
            ui.painter().line_segment([egui::pos2(playhead_x, rect.top()), egui::pos2(playhead_x, rect.bottom())], Stroke::new(1.0, Color32::from_rgb(0, 255, 200)));
        }
    });

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label("Waveform Zoom:");
        ui.add(egui::Slider::new(&mut app.sampler_waveform_zoom, 1.0..=32.0).logarithmic(true));
    });

    ui.add_space(20.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.heading("Capture & Recording");
            ui.add_space(10.0);
            Frame::group(ui.style()).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Input Gain");
                        let mut gain = 1.0;
                        if ui.add(egui::Slider::new(&mut gain, 0.0..=4.0)).changed() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: 110,
                                param_id: 0,
                                value: gain,
                                ramp_duration_samples: 0,
                            }));
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Monitor");
                        let mut monitor = 0.0;
                        if ui.add(egui::Slider::new(&mut monitor, 0.0..=1.0)).changed() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: 110,
                                param_id: 1,
                                value: monitor,
                                ramp_duration_samples: 0,
                            }));
                        }
                    });
                    ui.horizontal(|ui| {
                        let mut stereo = true;
                        if ui.checkbox(&mut stereo, "Stereo").changed() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: 110,
                                param_id: 2,
                                value: if stereo { 1.0 } else { 0.0 },
                                ramp_duration_samples: 0,
                            }));
                        }

                        if ui.button("RESET BUFFER").clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: 110,
                                param_id: 4,
                                value: 1.0,
                                ramp_duration_samples: 0,
                            }));
                        }
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let is_rec = false;
                        if ui.add(egui::Button::new(RichText::new("● RECORD").color(if is_rec { Color32::RED } else { Color32::from_rgb(150, 0, 0) })).min_size(Vec2::new(100.0, 30.0))).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: 110,
                                param_id: 3,
                                value: 1.0,
                                ramp_duration_samples: 0,
                            }));
                        }
                        if ui.button("STOP").clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: 110,
                                param_id: 3,
                                value: 0.0,
                                ramp_duration_samples: 0,
                            }));
                        }
                    });
                    ui.add_space(10.0);
                    if ui.button("COMMIT TO LIBRARY").clicked() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::RegisterCapture {
                            capture_node_idx: 110,
                            sample_id: 0, // Placeholder or generated
                        }));
                    }
                });
            });
        });

        ui.add_space(30.0);

        ui.vertical(|ui| {
            ui.heading("Loop Slicer");
            ui.add_space(10.0);
            if ui.checkbox(&mut app.sampler_slicer_mode, "ENABLE SLICER").changed() {
                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                    target_id: 100,
                    param_id: 3,
                    value: if app.sampler_slicer_mode { 1.0 } else { 0.0 },
                    ramp_duration_samples: 0,
                }));
            }
            ui.add_space(10.0);
            ui.group(|ui| {
                ui.label(RichText::new("PERFORMANCE SLICE PADS").small().color(Color32::from_gray(100)));
                egui::Grid::new("slicer_pads").spacing([10.0, 10.0]).show(ui, |ui| {
                    for row in 0..2 {
                        for col in 0..8 {
                            let idx = row * 8 + col;
                            let btn = egui::Button::new(RichText::new(format!("{}", idx + 1)).strong())
                                .min_size(Vec2::splat(45.0))
                                .fill(Color32::from_gray(40));

                            if ui.add(btn).clicked() {
                                let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::TriggerSlice {
                                    node_idx: 100,
                                    slice_idx: idx as u32,
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
