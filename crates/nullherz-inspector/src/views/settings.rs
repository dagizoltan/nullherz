use egui::{Ui, Color32, Frame, RichText, Vec2, Sense};
use crate::{InspectorApp, SettingsTab};
use nullherz_traits::AudioBackendType;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.heading("System Settings");
    ui.add_space(theme.space_md);

    // Two-Column Vertical Tab Layout
    ui.columns(2, |columns| {
        // Left column (width adjusted for sidebar navigation)
        let left_ui = &mut columns[0];
        left_ui.set_max_width(180.0);
        left_ui.vertical(|ui| {
            ui.label(RichText::new("SECTIONS").small().strong().color(theme.text_secondary));
            ui.add_space(theme.space_xs);

            let options = [
                (SettingsTab::General, "⚙ GENERAL"),
                (SettingsTab::Audio, "🔊 AUDIO"),
                (SettingsTab::Midi, "🎹 MIDI"),
                (SettingsTab::Network, "🌐 NETWORK"),
                (SettingsTab::Calibration, "📐 CALIBRATION"),
            ];

            nullherz_ui_hal::widgets::render_segmented_control_vertical(
                ui,
                &theme,
                &mut app.active_settings_tab,
                &options,
                160.0,
            );

            ui.add_space(theme.space_lg);
            if ui.button(RichText::new("🛑 STOP ENGINE").strong().color(theme.danger)).clicked() {
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop));
            }
        });

        // Right column (scrollable settings pane)
        let right_ui = &mut columns[1];
        egui::ScrollArea::vertical().id_source("settings_pane_scroll").show(right_ui, |ui| {
            ui.set_min_width(380.0);
            match app.active_settings_tab {
                SettingsTab::General => render_general(app, ui),
                SettingsTab::Audio => render_audio(app, ui),
                SettingsTab::Midi => render_midi(app, ui),
                SettingsTab::Network => render_network(app, ui),
                SettingsTab::Calibration => render_calibration(app, ui),
            }
        });
    });

    ui.add_space(theme.space_lg);
    ui.separator();
    ui.add_space(theme.space_md);

    // PERSISTENT FOOTER: "SAVE SYSTEM CONFIG" with transient feedback
    ui.horizontal(|ui| {
        let save_btn = egui::Button::new(RichText::new("💾 SAVE SYSTEM CONFIG").strong().size(theme.type_label))
            .fill(theme.accent.linear_multiply(0.15));

        if ui.add_sized([200.0, 32.0], save_btn).clicked() {
             let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology));
             // Trigger actual persistence in conductor
             let ports = "Pioneer DDJ-400,Generic MIDI Keyboard".to_string();
             let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMidiPorts({
                 let mut b = [0u8; 128];
                 let bytes = ports.as_bytes();
                 b[..bytes.len().min(128)].copy_from_slice(&bytes[..bytes.len().min(128)]);
                 b
             })));
             app.config_saved_time = Some(ui.input(|i| i.time));
        }

        if let Some(saved_t) = app.config_saved_time {
            let current_t = ui.input(|i| i.time);
            if current_t - saved_t < 1.5 {
                ui.label(RichText::new("Saved ✓").strong().color(theme.success));
                ui.ctx().request_repaint(); // Keep repainting while banner is active
            } else {
                app.config_saved_time = None;
            }
        }
    });
}

fn render_general(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("Global Transport");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Global BPM:");
                if ui.add(egui::DragValue::new(&mut app.global_bpm).clamp_range(40.0..=300.0)).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetBpm(app.global_bpm)));
                }
            });
            ui.add_space(theme.space_sm);
            if ui.checkbox(&mut app.quantize_enabled, "Quantize Commands").changed() {
                 let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetSafeMode(app.quantize_enabled)));
            }
        });
}

fn render_audio(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("Audio Engine Configuration");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("Select Audio Backend").color(theme.text_secondary));
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                let backends = [
                    (AudioBackendType::Alsa, "ALSA"),
                    (AudioBackendType::Jack, "JACK"),
                    (AudioBackendType::Pipewire, "Pipewire"),
                    (AudioBackendType::Threaded, "Threaded"),
                ];

                for (backend, label) in backends {
                    let is_active = app.active_backend == backend;
                    let mut btn = egui::Button::new(label);
                    if is_active {
                        btn = btn.fill(theme.accent.linear_multiply(0.12))
                                 .stroke(egui::Stroke::new(1.0, theme.accent));
                    }
                    if ui.add(btn).clicked() {
                        app.active_backend = backend;
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(backend)));
                    }
                }
            });

            ui.add_space(theme.space_md);
            ui.horizontal(|ui| {
                ui.label("Device:");
                egui::ComboBox::from_id_source("audio_device_select")
                    .selected_text(&app.selected_audio_device)
                    .show_ui(ui, |ui| {
                        for dev in &app.audio_devices {
                            ui.selectable_value(&mut app.selected_audio_device, dev.clone(), dev);
                        }
                    });

                if ui.button("Scan for Devices").clicked() {
                     // Trigger re-enumeration in backend (noop for now as orchestrator does it every tick)
                }
            });
        });

    ui.add_space(theme.space_md);
    ui.strong("Soundcard Wiring Test");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("Verify the physical outputs and routing of your audio interface by outputting a test signal.").color(theme.text_secondary));
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                if ui.button("▶ PLAY TEST PREVIEW (TRACK A)").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::Preview { sample_id: 1 }));
                }
                if ui.button("▶ PLAY TEST PREVIEW (TRACK B)").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::Preview { sample_id: 2 }));
                }
                if ui.button("⏹ STOP TEST").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::StopNode {
                        node_idx: nullherz_traits::NodeConventions::PREVIEW,
                    }));
                }
            });
        });
}

fn render_midi(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("MIDI Hardware & Mappings");
    ui.add_space(theme.space_xs);

    // Mock/planned disclosure for live MIDI device enumeration
    ui.horizontal(|ui| {
        ui.label(RichText::new("ℹ NOTE: MIDI Device enumeration is currently running in Mock/Simulated mode. Actual RT MIDI port mapping auto-discovery is not yet fully exposed by the midi_mapper.rs backend.").size(9.0).color(theme.text_secondary));
    });
    ui.add_space(theme.space_xs);

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label("Active Port Mappings:");
            ui.label("• Pioneer DDJ-400 (Attached)");
            ui.label("• Generic MIDI Keyboard (Attached)");
            ui.add_space(theme.space_sm);

            if ui.button("BIND DETECTED PORTS").clicked() {
                let ports = "Pioneer DDJ-400,Generic MIDI Keyboard";
                let mut buffer = [0u8; 128];
                let bytes = ports.as_bytes();
                let len = bytes.len().min(128);
                buffer[..len].copy_from_slice(&bytes[..len]);
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMidiPorts(buffer)));
            }

            ui.add_space(theme.space_md);
            ui.label("Controller Profiles:");
            ui.add_space(theme.space_xs);
            ui.horizontal(|ui| {
                let options = ["default", "pioneer_ddj400", "akai_mpk_mini"];
                for opt in options {
                    let is_active = app.active_midi_profile == opt;
                    let mut btn = egui::Button::new(format!("Load {}", opt));
                    if is_active {
                        btn = btn.fill(theme.accent.linear_multiply(0.12))
                                 .stroke(egui::Stroke::new(1.0, theme.accent));
                    }
                    if ui.add(btn).clicked() {
                        app.active_midi_profile = opt.to_string();
                        let mut buffer = [0u8; 32];
                        let bytes = opt.as_bytes();
                        let len = bytes.len().min(32);
                        buffer[..len].copy_from_slice(&bytes[..len]);
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::LoadMidiMap(buffer)));
                    }
                }
            });
        });
}

fn render_network(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("Distributed Sidecar Discovery");
    ui.add_space(theme.space_xs);

    // Mock/planned disclosure for remote node discovery
    ui.horizontal(|ui| {
        ui.label(RichText::new("ℹ NOTE: P2P Network Node Discovery is currently running in Mock/Simulated mode. Actual mesh/gossip node discovery is not yet fully exposed by the discovery.rs network backend.").size(9.0).color(theme.text_secondary));
    });
    ui.add_space(theme.space_xs);

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("P2P Cloud Sync and Remote DSP Nodes").color(theme.text_secondary));
            ui.add_space(theme.space_md);

            ui.label("Remote Nodes Detected:");
            ui.add_space(theme.space_sm);

            let remote_nodes = [
                ("192.168.1.45 (Studio-PC-2)", true),
                ("192.168.1.12 (MacBook-Pro-DSP)", false),
            ];

            for (name, is_connected) in remote_nodes {
                Frame::none()
                    .fill(theme.bg_inset)
                    .rounding(theme.radius_md)
                    .stroke(theme.border_stroke)
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Status indicator dot
                            let dot_color = if is_connected { theme.success } else { theme.danger };
                            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                            ui.painter().circle_filled(dot_rect.center(), 4.0, dot_color);
                            ui.add_space(theme.space_xs);

                            ui.label(name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(if is_connected { "DISCONNECT" } else { "ATTACH" }).clicked() {
                                    println!("Toggling node connection...");
                                }
                            });
                        });
                    });
                ui.add_space(theme.space_sm);
            }
        });
}

fn render_calibration(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("Hardware Latency Calibration");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label("Measure Round-Trip Latency (RTL) to ensure sample-accurate alignment.");
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                if ui.button(RichText::new("● START CALIBRATION").color(theme.accent)).clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CalibrateLatency));
                }

                if let Some(t) = app.last_telemetry.lock().unwrap().as_ref() {
                    if t.calibration_samples > 0 {
                        let ms = t.calibration_samples as f32 / (t.sample_rate / 1000.0) ;
                        ui.label(format!("Current RTL: {:.1}ms ({} samples)", ms, t.calibration_samples));
                    } else {
                        ui.label("Current RTL: Not Calibrated");
                    }
                } else {
                    ui.label("Current RTL: --");
                }
            });
        });

    ui.add_space(theme.space_md);
    ui.strong("Distributed Clock Discipline (PTP/IEEE 1588)");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            if let Some(t) = app.last_telemetry.lock().unwrap().as_ref() {
                ui.horizontal(|ui| {
                    ui.label("Sync Status:");
                    if t.clock_jitter_ns < 1000 {
                        ui.label(RichText::new("● LOCKED").color(theme.success));
                    } else {
                        ui.label(RichText::new("○ SEEKING").color(theme.warning));
                    }
                });
                ui.label(format!("System Time: {} ns", t.system_time_ns));
                ui.label(format!("Device Time: {} ns", t.device_time_ns));
                ui.label(format!("Jitter: {} ns", t.clock_jitter_ns));
                ui.label(format!("Offset: {} ns", (t.device_time_ns as i64 - t.system_time_ns as i64)));
            } else {
                ui.label("No clock telemetry available.");
            }
        });
}
