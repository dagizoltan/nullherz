use egui::{Ui, Frame, Vec2, Sense, RichText, Rounding, Stroke, Margin, Pos2};
use crate::InspectorApp;
use crate::state::{ChannelInputSource, MasterOutput};
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;
use nullherz_traits::{Command, CoreCommand, MidiEvent, MixerCommand};

const SUBCHANNEL_LABELS: [&str; 16] = [
    "KICK", "SNARE", "HH-CL", "HH-OP",
    "TOM-LO", "TOM-MID", "TOM-HI", "PERC 1",
    "PERC 2", "CLAP", "RIDE", "CRASH",
    "FX 1", "FX 2", "AUX 1", "AUX 2",
];

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let focus = app.mixer.focused_detail_channel;
    let is_master = focus == 20;
    let is_drum = (4..20).contains(&focus);
    let deck_i = if is_drum { focus - 4 } else if is_master { 0 } else { focus };
    let track_color = if is_master {
        app.theme.accent
    } else {
        crate::InspectorApp::deck_color(&app.theme, deck_i % 4)
    };

    // 1. TOP CHANNEL SWITCHER TABS
    Frame::none()
        .fill(app.theme.bg_surface)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_sm)
        .show(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("CHANNEL INSPECTOR:").strong().size(app.theme.type_caption).color(app.theme.text_secondary));

                    // Main Channels 0..3 (CH A..D)
                    for i in 0..4 {
                        let ch_name = format!("CH {}", (b'A' + i as u8) as char);
                        let is_sel = app.mixer.focused_detail_channel == i;
                        let btn_col = if is_sel { crate::InspectorApp::deck_color(&app.theme, i) } else { app.theme.bg_inset };
                        if ui.add(egui::Button::new(RichText::new(ch_name).strong().size(9.0)).fill(btn_col)).clicked() {
                            app.mixer.focused_detail_channel = i;
                        }
                    }

                    ui.separator();

                    // Drum Subchannels 4..19 (DRUM 01..16)
                    for d_i in 0..16 {
                        let ch_idx = d_i + 4;
                        let label = format!("D{:02} {}", d_i + 1, SUBCHANNEL_LABELS[d_i]);
                        let is_sel = app.mixer.focused_detail_channel == ch_idx;
                        let btn_col = if is_sel { crate::InspectorApp::deck_color(&app.theme, d_i % 4) } else { app.theme.bg_inset };
                        if ui.add(egui::Button::new(RichText::new(label).strong().size(8.0)).fill(btn_col)).clicked() {
                            app.mixer.focused_detail_channel = ch_idx;
                        }
                    }

                    ui.separator();

                    // Master Channel (20)
                    let is_sel = app.mixer.focused_detail_channel == 20;
                    let m_btn_col = if is_sel { app.theme.accent } else { app.theme.bg_inset };
                    if ui.add(egui::Button::new(RichText::new("MASTER").strong().size(9.0)).fill(m_btn_col)).clicked() {
                        app.mixer.focused_detail_channel = 20;
                    }
                });
            });
        });

    ui.add_space(8.0);

    // 2. MAIN DETAIL GRID (3-COLUMN LAYOUT)
    ui.horizontal_top(|ui| {
        // COLUMN 1: SIGNAL ROUTING, GAIN STAGING & FADER
        Frame::none()
            .fill(app.theme.bg_surface)
            .rounding(app.theme.radius_md)
            .stroke(app.theme.border_stroke)
            .inner_margin(app.theme.space_md)
            .show(ui, |ui| {
                ui.set_width(220.0);
                ui.vertical_centered(|ui| {
                    let title = if is_master {
                        "MASTER BUS".to_string()
                    } else if is_drum {
                        format!("SUBCHANNEL {:02}: {}", deck_i + 1, SUBCHANNEL_LABELS[deck_i])
                    } else {
                        format!("CHANNEL {}", (b'A' + focus as u8) as char)
                    };

                    ui.label(RichText::new(title).strong().size(app.theme.type_heading).color(track_color));
                    ui.add_space(6.0);

                    // Input Source Selector
                    if !is_master {
                        ui.label(RichText::new("INPUT SOURCE").size(8.0).color(app.theme.text_secondary));
                        let mut current_src = app.mixer.channel_input_sources[deck_i % 16];
                        egui::ComboBox::from_id_source(format!("detail_in_src_{}", focus))
                            .selected_text(RichText::new(current_src.name()).size(9.0).strong())
                            .width(200.0)
                            .show_ui(ui, |ui| {
                                for src in ChannelInputSource::all() {
                                    if ui.selectable_value(&mut current_src, *src, src.name()).clicked() {
                                        app.mixer.channel_input_sources[deck_i % 16] = *src;
                                    }
                                }
                            });
                        ui.add_space(8.0);
                    }

                    // Rotary Controls for Gain Staging
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let mut trim_val = if is_drum { app.sampler.subchannel_gain[deck_i] } else { app.mixer.channel_gain[deck_i % 16] };
                        if widgets::knobs::render_knob_sized(ui, &mut trim_val, 0.0..=2.0, "TRIM", track_color, 32.0).changed() {
                            if is_drum { app.sampler.subchannel_gain[deck_i] = trim_val; } else { app.mixer.channel_gain[deck_i % 16] = trim_val; }
                        }

                        let mut pan_val = app.mixer.channel_balance[deck_i % 16];
                        if widgets::knobs::render_knob_sized(ui, &mut pan_val, 0.0..=1.0, "PAN", track_color, 32.0).changed() {
                            app.mixer.channel_balance[deck_i % 16] = pan_val;
                        }

                        let mut width_val = app.mixer.channel_width[deck_i % 16];
                        if widgets::knobs::render_knob_sized(ui, &mut width_val, 0.0..=2.0, "WIDTH", track_color, 32.0).changed() {
                            app.mixer.channel_width[deck_i % 16] = width_val;
                        }
                    });

                    ui.add_space(8.0);

                    // Mute / Solo Toggles
                    ui.horizontal(|ui| {
                        let mut mute = if is_drum { app.sampler.subchannel_mutes[deck_i] } else { app.mixer.stem_mutes[0][deck_i % 12] };
                        let mute_bg = if mute { app.theme.danger } else { app.theme.bg_inset };
                        if ui.add_sized([90.0, 24.0], egui::Button::new(RichText::new("MUTE").strong()).fill(mute_bg)).clicked() {
                            mute = !mute;
                            if is_drum { app.sampler.subchannel_mutes[deck_i] = mute; } else { app.mixer.stem_mutes[0][deck_i % 12] = mute; }
                        }

                        let mut solo = if is_drum { app.sampler.subchannel_solos[deck_i] } else { app.mixer.stem_solos[0][deck_i % 12] };
                        let solo_bg = if solo { app.theme.warning } else { app.theme.bg_inset };
                        if ui.add_sized([90.0, 24.0], egui::Button::new(RichText::new("SOLO").strong()).fill(solo_bg)).clicked() {
                            solo = !solo;
                            if is_drum { app.sampler.subchannel_solos[deck_i] = solo; } else { app.mixer.stem_solos[0][deck_i % 12] = solo; }
                        }
                    });

                    ui.add_space(10.0);

                    // Volume Fader & Stereo Peak VU Meter
                    ui.horizontal(|ui| {
                        let fader_w = 28.0;
                        ui.add_space(40.0);

                        let mut fader_val = if is_master {
                            app.mixer.master_gain
                        } else if is_drum {
                            app.sampler.subchannel_faders[deck_i]
                        } else {
                            app.mixer.channel_faders[deck_i % 16]
                        };

                        if widgets::render_fader(ui, &mut fader_val, 0.0..=1.2, track_color, 180.0, 32.0).changed() {
                            if is_master {
                                app.mixer.master_gain = fader_val;
                            } else if is_drum {
                                app.sampler.subchannel_faders[deck_i] = fader_val;
                            } else {
                                app.mixer.channel_faders[deck_i % 16] = fader_val;
                            }
                        }

                        ui.add_space(10.0);

                        let peak_lvl = telemetry.as_ref().and_then(|t| t.peak_levels.get(deck_i)).copied().unwrap_or(0.0);
                        widgets::render_vu_meter(ui, peak_lvl, 0.0, track_color, 180.0);
                    });
                });
            });

        ui.add_space(8.0);

        // COLUMN 2: PRIMARY GENERATOR / INSTRUMENT UI
        Frame::none()
            .fill(app.theme.bg_surface)
            .rounding(app.theme.radius_md)
            .stroke(app.theme.border_stroke)
            .inner_margin(app.theme.space_md)
            .show(ui, |ui| {
                ui.set_width(360.0);
                ui.vertical(|ui| {
                    ui.heading(RichText::new("PRIMARY INSTRUMENT / GENERATOR").strong().size(app.theme.type_body));
                    ui.add_space(6.0);

                    if is_drum {
                        // Analog / Sample / Neural Drum Subchannel Generator Inspector
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("DRUM VOICE GENERATOR").strong().color(app.theme.accent));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(RichText::new("▶ TRIGGER").strong().color(app.theme.text_primary)).clicked() {
                                    let note = (36 + deck_i) as u8;
                                    let event = MidiEvent { timestamp_samples: 0, status: 0x90, data1: note, data2: 120, _pad: 0 };
                                    let _ = app.command_sender.send(Command::Core(CoreCommand::InjectMidi(event)));
                                }
                            });
                        });
                        ui.add_space(6.0);

                        Frame::none()
                            .fill(app.theme.bg_dark)
                            .rounding(Rounding::same(app.theme.radius_sm))
                            .stroke(Stroke::new(1.0, track_color))
                            .inner_margin(Margin::same(8.0))
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new("DSP PARAMETER SYNTHESIS").strong().size(9.0).color(app.theme.text_secondary));
                                    ui.add_space(6.0);

                                    egui::Grid::new("channel_detail_synth_grid")
                                        .num_columns(2)
                                        .spacing([12.0, 8.0])
                                        .show(ui, |ui| {
                                            ui.label("Pitch / Tune");
                                            let mut tune = app.sampler.pad_tune[deck_i];
                                            if ui.add(egui::Slider::new(&mut tune, 0.0..=1.0)).changed() {
                                                app.sampler.pad_tune[deck_i] = tune;
                                                if let Some(dm) = app.get_node_id("drum_machine_node") {
                                                    let _ = app.command_sender.send(Command::Mixer(MixerCommand::SetParam {
                                                        target_id: dm as u64, param_id: (deck_i * 16 + 0) as u32, value: tune, ramp_duration_samples: 0,
                                                    }));
                                                }
                                            }
                                            ui.end_row();

                                            ui.label("Decay Envelope");
                                            let mut decay = app.sampler.pad_decay[deck_i];
                                            if ui.add(egui::Slider::new(&mut decay, 0.0..=1.0)).changed() {
                                                app.sampler.pad_decay[deck_i] = decay;
                                                if let Some(dm) = app.get_node_id("drum_machine_node") {
                                                    let _ = app.command_sender.send(Command::Mixer(MixerCommand::SetParam {
                                                        target_id: dm as u64, param_id: (deck_i * 16 + 1) as u32, value: decay, ramp_duration_samples: 0,
                                                    }));
                                                }
                                            }
                                            ui.end_row();

                                            ui.label("Pitch Sweep");
                                            let mut sweep = app.sampler.pad_sweep[deck_i];
                                            if ui.add(egui::Slider::new(&mut sweep, 0.0..=1.0)).changed() {
                                                app.sampler.pad_sweep[deck_i] = sweep;
                                                if let Some(dm) = app.get_node_id("drum_machine_node") {
                                                    let _ = app.command_sender.send(Command::Mixer(MixerCommand::SetParam {
                                                        target_id: dm as u64, param_id: (deck_i * 16 + 2) as u32, value: sweep, ramp_duration_samples: 0,
                                                    }));
                                                }
                                            }
                                            ui.end_row();

                                            ui.label("Padé Drive");
                                            let mut drive = app.sampler.pad_drive[deck_i];
                                            if ui.add(egui::Slider::new(&mut drive, 0.0..=1.0)).changed() {
                                                app.sampler.pad_drive[deck_i] = drive;
                                                if let Some(dm) = app.get_node_id("drum_machine_node") {
                                                    let _ = app.command_sender.send(Command::Mixer(MixerCommand::SetParam {
                                                        target_id: dm as u64, param_id: (deck_i * 16 + 4) as u32, value: drive, ramp_duration_samples: 0,
                                                    }));
                                                }
                                            }
                                            ui.end_row();
                                        });
                                });
                            });
                    } else if let Some(ref track) = app.decks.cached_tracks.get(deck_i % 4).and_then(|t| t.as_ref()) {
                        // High-Resolution Waveform Canvas Inspector for Decks A..D
                        let (rect, _response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 160.0), Sense::hover());
                        ui.painter().rect_filled(rect, app.theme.radius_sm, app.theme.bg_dark);
                        ui.painter().rect_stroke(rect, app.theme.radius_sm, Stroke::new(1.0, track_color));

                        if let (Some(wgpu_mtx), Some(wf_mtx)) = (&app.wgpu_renderer, &app.waveform_renderer) {
                            let _wgpu = wgpu_mtx.lock();
                            let mut wf = wf_mtx.lock();
                            let color = track_color.to_array().map(|v| v as f32 / 255.0);
                            wf.update_from_mip_waveform(&_wgpu.queue, &track.metadata.mip_waveform, 1.0, rect.width() as u32, color);
                            nullherz_ui_hal::render::waveform_renderer::ui_paint_waveform(ui, rect, wf_mtx.clone());
                        }

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&track.title).strong().size(11.0).color(app.theme.text_primary));
                            ui.label(RichText::new(format!("{:.1} BPM", track.metadata.bpm)).monospace().size(10.0).color(track_color));
                        });
                    } else {
                        ui.label(RichText::new("No Deck Track Loaded").italics().color(app.theme.text_disabled));
                    }
                });
            });

        ui.add_space(8.0);

        // COLUMN 3: EXPANDED MODULAR INSERTS RACK & PERCEPTUAL ANALYZER
        Frame::none()
            .fill(app.theme.bg_surface)
            .rounding(app.theme.radius_md)
            .stroke(app.theme.border_stroke)
            .inner_margin(app.theme.space_md)
            .show(ui, |ui| {
                ui.set_width(320.0);
                ui.vertical(|ui| {
                    ui.heading(RichText::new("EXPANDED INSERTS RACK & ANALYZER").strong().size(app.theme.type_body));
                    ui.add_space(6.0);

                    // Insert Rack Items List
                    let inserts = if is_drum {
                        app.sampler.subchannel_inserts[deck_i].clone()
                    } else {
                        app.decks.deck_inserts[deck_i % 4].clone()
                    };

                    let mut fx_to_remove = None;
                    let mut fx_to_move_up = None;
                    let mut fx_to_move_down = None;
                    let total_fx = inserts.len();

                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        ui.vertical(|ui| {
                            for (fx_i, fx_name) in inserts.iter().enumerate() {
                                ui.push_id(fx_i, |ui| {
                                    Frame::none()
                                        .fill(app.theme.bg_inset)
                                        .rounding(Rounding::same(app.theme.radius_sm))
                                        .stroke(Stroke::new(1.0, track_color))
                                        .inner_margin(Margin::same(6.0))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new(format!("{}: {}", fx_i + 1, fx_name)).strong().size(10.0).color(app.theme.text_primary));
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    if ui.button(RichText::new("×").strong().color(app.theme.danger)).clicked() {
                                                        fx_to_remove = Some(fx_i);
                                                    }
                                                    if fx_i < total_fx - 1 {
                                                        if ui.button("▼").clicked() { fx_to_move_down = Some(fx_i); }
                                                    }
                                                    if fx_i > 0 {
                                                        if ui.button("▲").clicked() { fx_to_move_up = Some(fx_i); }
                                                    }
                                                });
                                            });
                                        });
                                    ui.add_space(4.0);
                                });
                            }
                        });
                    });

                    if let Some(idx) = fx_to_move_up {
                        if idx > 0 {
                            if is_drum { app.sampler.subchannel_inserts[deck_i].swap(idx, idx - 1); }
                            else { app.decks.deck_inserts[deck_i % 4].swap(idx, idx - 1); }
                        }
                    }
                    if let Some(idx) = fx_to_move_down {
                        if idx + 1 < total_fx {
                            if is_drum { app.sampler.subchannel_inserts[deck_i].swap(idx, idx + 1); }
                            else { app.decks.deck_inserts[deck_i % 4].swap(idx, idx + 1); }
                        }
                    }
                    if let Some(remove_i) = fx_to_remove {
                        if is_drum && remove_i < app.sampler.subchannel_inserts[deck_i].len() {
                            app.sampler.subchannel_inserts[deck_i].remove(remove_i);
                        } else if !is_drum && remove_i < app.decks.deck_inserts[deck_i % 4].len() {
                            app.decks.deck_inserts[deck_i % 4].remove(remove_i);
                        }
                    }

                    ui.add_space(6.0);
                    if ui.button(RichText::new("+ ADD SIDECAR INSERT").strong().color(app.theme.accent)).clicked() {
                        app.active_right_tab = Some(crate::RightTab::Store);
                        app.store.active_category = Some(sidecar_sdk::AssetCategory::AudioInsert);
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // Mini Spectrum & Phase Visualizer
                    ui.label(RichText::new("CHANNEL PERCEPTUAL ACOUSTIC SPECTRUM").strong().size(9.0).color(app.theme.text_secondary));
                    let (spec_rect, _resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 80.0), Sense::hover());
                    ui.painter().rect_filled(spec_rect, app.theme.radius_sm, app.theme.bg_dark);

                    if let Some(t) = telemetry {
                        let bins = 32;
                        let bar_w = spec_rect.width() / bins as f32;
                        for b in 0..bins {
                            let val = t.spectrum.get(b * 4).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                            let bar_h = val * spec_rect.height();
                            let x = spec_rect.min.x + b as f32 * bar_w;
                            let y0 = spec_rect.max.y;
                            let y1 = spec_rect.max.y - bar_h;
                            ui.painter().line_segment([Pos2::new(x + bar_w * 0.5, y0), Pos2::new(x + bar_w * 0.5, y1)], Stroke::new(bar_w * 0.8, track_color));
                        }
                    }
                });
            });
    });
}
