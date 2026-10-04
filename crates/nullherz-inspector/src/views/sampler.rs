use egui::{Ui, Frame, Vec2, Sense, RichText, Rounding, Stroke, Margin};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;
use nullherz_traits::{Command, CoreCommand, MidiEvent, TopologyCommand, MixerCommand};
use nullherz_dna::{GeneticLibrary, SampleDrumKitPreset, SynthDrumKitPreset, NeuralDrumKitPreset};

const SUBCHANNEL_LABELS: [&str; 16] = [
    "KICK", "SNARE", "HH-CL", "HH-OP",
    "TOM-LO", "TOM-MID", "TOM-HI", "PERC 1",
    "PERC 2", "CLAP", "RIDE", "CRASH",
    "FX 1", "FX 2", "AUX 1", "AUX 2",
];

#[allow(clippy::collapsible_if)]
pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        ui.heading("Production Sampler & 16-Channel Drum Submixer");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.sampler.sampler_is_recording {
                let time = ui.input(|i| i.time);
                let alpha = ((time * 3.0).sin() * 0.5 + 0.5) as f32;
                ui.label(RichText::new("● RECORDING").color(app.theme.danger.gamma_multiply(alpha)).strong());
            }
        });
    });
    ui.add_space(6.0);

    // Waveform Preview Area
    Frame::none()
        .fill(app.theme.bg_dark)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_md)
        .show(ui, |ui| {
            let (rect, _response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 100.0), Sense::hover());

            if let (Some(wgpu_mtx), Some(wf_mtx)) = (&app.wgpu_renderer, &app.waveform_renderer) {
                 let _wgpu = wgpu_mtx.lock();
                 let mut wf = wf_mtx.lock();

                 if let Some(track_id) = app.sampler.source_track {
                     if let Some(track) = app.get_cached_track(track_id) {
                         wf.update_from_mip_waveform(&_wgpu.queue, &track.metadata.mip_waveform, app.sampler.sampler_waveform_zoom, rect.width() as u32, app.theme.accent.to_array().map(|v| v as f32 / 255.0));
                     }
                 }

                 let color = app.theme.accent.to_array().map(|v| v as f32 / 255.0);
                 wf.update_globals(&_wgpu.queue, 0.0, app.sampler.sampler_waveform_zoom, false, app.mixer.waveform_styles[0], color);

                 nullherz_ui_hal::render::waveform_renderer::ui_paint_waveform(ui, rect, wf_mtx.clone());
            }
        });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Zoom:");
        ui.add(egui::Slider::new(&mut app.sampler.sampler_waveform_zoom, 1.0..=32.0).logarithmic(true).show_value(false));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
             if ui.button("RESET VIEW").clicked() { app.sampler.sampler_waveform_zoom = 1.0; }
        });
    });

    ui.add_space(8.0);

    // 16-PAD DRUM MACHINE MATRIX & KIT PRESETS
    Frame::none()
        .fill(app.theme.bg_surface)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_sm)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("DRUM MATRIX").strong().size(app.theme.type_body));
                ui.add_space(10.0);

                // Kit Preset Hot-loading
                ui.label(RichText::new("HOTLOAD KIT:").strong().size(app.theme.type_caption).color(app.theme.text_secondary));
                if ui.add(egui::Button::new(RichText::new("SAMPLE KIT").size(app.theme.type_caption).strong()).fill(app.theme.accent.linear_multiply(0.2))).clicked() {
                    let preset = SampleDrumKitPreset::default();
                    for pad in &preset.pads {
                        let idx = pad.pad_index as usize;
                        if idx < 16 {
                            if let Some(ref path) = pad.sample_path {
                                app.composer.track_targets[idx] = path.to_string();
                            }
                        }
                    }
                }
                if ui.add(egui::Button::new(RichText::new("ANALOG SYNTH KIT").size(app.theme.type_caption).strong()).fill(app.theme.warning.linear_multiply(0.2))).clicked() {
                    let _preset = SynthDrumKitPreset::default();
                }
                if ui.add(egui::Button::new(RichText::new("CORTICAL NEURAL KIT").size(app.theme.type_caption).strong()).fill(app.theme.success.linear_multiply(0.2))).clicked() {
                    let _preset = NeuralDrumKitPreset::default();
                }
            });

            ui.add_space(6.0);

            // 4x4 Pad Grid Layout
            egui::Grid::new("drum_pads_4x4_grid")
                .num_columns(4)
                .spacing([6.0, 6.0])
                .show(ui, |ui| {
                    for row in 0..4 {
                        for col in 0..4 {
                            let pad_idx = row * 4 + col;
                            let note = (36 + pad_idx) as u8;
                            let deck_color = crate::InspectorApp::deck_color(&app.theme, pad_idx % 4);

                            Frame::none()
                                .fill(deck_color.gamma_multiply(0.12))
                                .rounding(Rounding::same(app.theme.radius_sm))
                                .stroke(Stroke::new(1.0, deck_color))
                                .inner_margin(Margin::same(4.0))
                                .show(ui, |ui| {
                                    ui.set_width(110.0);
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(format!("P{:02} {}", pad_idx + 1, SUBCHANNEL_LABELS[pad_idx])).strong().size(9.0).color(app.theme.text_primary));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.label(RichText::new(format!("N{}", note)).size(7.0).color(app.theme.text_secondary));
                                            });
                                        });

                                        ui.add_space(1.0);

                                        // Trigger Button
                                        let trig_btn = egui::Button::new(RichText::new("▶ TRIGGER").size(9.0).strong().color(app.theme.text_primary))
                                            .fill(deck_color.linear_multiply(0.35));
                                        if ui.add_sized([102.0, 20.0], trig_btn).clicked() {
                                            let event = MidiEvent {
                                                timestamp_samples: 0,
                                                status: 0x90,
                                                data1: note,
                                                data2: 120,
                                                _pad: 0,
                                            };
                                            let _ = app.command_sender.send(Command::Core(CoreCommand::InjectMidi(event)));
                                        }

                                        ui.add_space(1.0);

                                        // Sample Picker Dropdown per pad
                                        let current_src = app.composer.track_sources[pad_idx];
                                        let cached_track = current_src.and_then(|id| app.get_cached_track(id));
                                        let label = cached_track.as_ref()
                                            .map(|t| format!("♪ {}", t.title))
                                            .unwrap_or_else(|| "⊕ SAMPLE".to_string());

                                        egui::ComboBox::from_id_source(format!("pad_sample_sel_{}", pad_idx))
                                            .width(102.0)
                                            .selected_text(RichText::new(&label).size(8.0).strong())
                                            .show_ui(ui, |ui| {
                                                if ui.selectable_label(current_src.is_none(), "(None)").clicked() {
                                                    app.composer.track_sources[pad_idx] = None;
                                                }
                                                for lib_track in &app.library.cached_library_raw {
                                                    let is_sel = current_src == Some(lib_track.id);
                                                    if ui.selectable_label(is_sel, format!("♪ {}", lib_track.title)).clicked() {
                                                        app.composer.track_sources[pad_idx] = Some(lib_track.id);
                                                        if let Some(dm_node) = app.get_node_id("drum_machine_node") {
                                                            let db = app.library_db.0.lock();
                                                            if let Some(_buffer) = db.get_track(lib_track.id).ok().flatten().map(|t| nullherz_traits::SampleBuffer::from(t.metadata.peaks.as_slice().to_vec())) {
                                                                let _ = app.command_sender.send(Command::Topology(TopologyCommand::AddNode {
                                                                    processor_type_id: nullherz_traits::ProcessorTypeId::SAMPLE_DRUM_MACHINE,
                                                                    node_idx: dm_node,
                                                                }));
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                    });
                                });
                        }
                        ui.end_row();
                    }
                });
        });

    ui.add_space(8.0);

    // COMPACT 16-CHANNEL DRUM SUBMIXER STRIPS
    Frame::none()
        .fill(app.theme.bg_surface)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_sm)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("16-CHANNEL DRUM SUBMIXER").strong().size(app.theme.type_body));
                ui.add_space(10.0);
                ui.label(RichText::new("COMPACT PIXEL-PERFECT SUBCHANNEL STRIPS (NO HORIZONTAL SCROLL)").size(app.theme.type_caption).color(app.theme.accent));
            });

            ui.add_space(6.0);

            // 16 Compact Channels side-by-side without horizontal scrolling
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;

                for sub_idx in 0..16 {
                    let track_color = crate::InspectorApp::deck_color(&app.theme, sub_idx % 4);
                    let label = SUBCHANNEL_LABELS[sub_idx];

                    Frame::none()
                        .fill(app.theme.bg_dark)
                        .rounding(Rounding::same(app.theme.radius_sm))
                        .stroke(Stroke::new(1.0, track_color))
                        .inner_margin(Margin::same(3.0))
                        .show(ui, |ui| {
                            ui.set_width(58.0);
                            ui.vertical_centered(|ui| {
                                // Channel Header Badge
                                ui.label(RichText::new(format!("{:02}", sub_idx + 1)).strong().size(8.0).color(track_color));
                                ui.label(RichText::new(label).strong().size(8.0).color(app.theme.text_primary));

                                ui.add_space(2.0);

                                // Mute / Solo Buttons
                                ui.horizontal(|ui| {
                                    let mut mute = app.mixer.stem_mutes[0][sub_idx];
                                    let mute_color = if mute { app.theme.danger } else { app.theme.bg_inset };
                                    if ui.add_sized([22.0, 14.0], egui::Button::new(RichText::new("M").size(7.0).strong()).fill(mute_color)).clicked() {
                                        mute = !mute;
                                        app.mixer.stem_mutes[0][sub_idx] = mute;
                                    }

                                    let mut solo = app.mixer.stem_solos[0][sub_idx];
                                    let solo_color = if solo { app.theme.warning } else { app.theme.bg_inset };
                                    if ui.add_sized([22.0, 14.0], egui::Button::new(RichText::new("S").size(7.0).strong()).fill(solo_color)).clicked() {
                                        solo = !solo;
                                        app.mixer.stem_solos[0][sub_idx] = solo;
                                    }
                                });

                                ui.add_space(2.0);

                                // Trim Gain
                                ui.label(RichText::new("TRIM").size(6.0).color(app.theme.text_secondary));
                                let mut gain_val = app.mixer.channel_gain[sub_idx];
                                if ui.add(egui::Slider::new(&mut gain_val, 0.0..=2.0).show_value(false)).changed() {
                                    app.mixer.channel_gain[sub_idx] = gain_val;
                                    if let Some(dm_node) = app.get_node_id("drum_machine_node") {
                                        let _ = app.command_sender.send(Command::Mixer(MixerCommand::SetParam {
                                            target_id: dm_node as u64,
                                            param_id: (sub_idx * 16 + 7) as u32,
                                            value: gain_val,
                                            ramp_duration_samples: 0,
                                        }));
                                    }
                                }

                                ui.add_space(2.0);

                                // Mini 3-Band EQ Sliders
                                ui.label(RichText::new("HI").size(6.0).color(app.theme.text_secondary));
                                ui.add(egui::Slider::new(&mut app.mixer.channel_eq_high[sub_idx], 0.0..=2.0).show_value(false));

                                ui.label(RichText::new("MID").size(6.0).color(app.theme.text_secondary));
                                ui.add(egui::Slider::new(&mut app.mixer.channel_eq_mid[sub_idx], 0.0..=2.0).show_value(false));

                                ui.label(RichText::new("LOW").size(6.0).color(app.theme.text_secondary));
                                ui.add(egui::Slider::new(&mut app.mixer.channel_eq_low[sub_idx], 0.0..=2.0).show_value(false));

                                ui.add_space(2.0);

                                // Vertical Fader
                                let mut fader_val = app.mixer.channel_faders[sub_idx];
                                if ui.add(egui::Slider::new(&mut fader_val, 0.0..=1.2).vertical().show_value(false)).changed() {
                                    app.mixer.channel_faders[sub_idx] = fader_val;
                                }

                                ui.add_space(2.0);

                                // Peak VU Meter
                                if let Some(t) = telemetry {
                                    let lvl = t.peak_levels.get(sub_idx).cloned().unwrap_or(0.0);
                                    widgets::render_vu_meter(ui, lvl, app.mixer.channel_peak_hold[sub_idx], track_color, 35.0);
                                }
                            });
                        });
                }
            });
        });

    ui.add_space(8.0);

    // Standalone Capture Controls Panel
    ui.vertical(|ui| {
        ui.heading("Standalone Capture & Sampling Settings");
        ui.add_space(6.0);
        Frame::none()
            .fill(app.theme.bg_surface)
            .rounding(app.theme.radius_md)
            .stroke(app.theme.border_stroke)
            .inner_margin(app.theme.space_md)
            .show(ui, |ui| {
                egui::Grid::new("capture_settings_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
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

                    ui.label("Monitor Level");
                    if ui.add(egui::Slider::new(&mut app.sampler.sampler_monitor_level, 0.0..=1.0)).changed() {
                        if let Some(resolved_node) = app.get_node_id("capture_node") {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: resolved_node as u64, param_id: 1, value: app.sampler.sampler_monitor_level, ramp_duration_samples: 0,
                            }));
                        }
                    }
                    ui.end_row();

                    ui.label("Format");
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

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let rec_btn = if app.sampler.sampler_is_recording {
                        egui::Button::new(RichText::new("■ STOP").strong().color(app.theme.text_primary)).fill(app.theme.danger)
                    } else {
                        egui::Button::new(RichText::new("● RECORD").strong().color(app.theme.text_primary)).fill(app.theme.danger)
                    };

                    if ui.add(rec_btn.min_size(Vec2::new(100.0, 28.0))).clicked() {
                        app.sampler.sampler_is_recording = !app.sampler.sampler_is_recording;
                        if let Some(resolved_node) = app.get_node_id("capture_node") {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: resolved_node as u64, param_id: 3, value: if app.sampler.sampler_is_recording { 1.0 } else { 0.0 }, ramp_duration_samples: 0,
                            }));
                        }
                    }

                    if ui.add(egui::Button::new("RESET").min_size(Vec2::new(60.0, 28.0))).clicked() {
                        if let Some(resolved_node) = app.get_node_id("capture_node") {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: resolved_node as u64, param_id: 4, value: 1.0, ramp_duration_samples: 0,
                            }));
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("COMMIT TO LIBRARY").strong().color(app.theme.accent)).clicked() {
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
}
