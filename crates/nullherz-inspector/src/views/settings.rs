use egui::{Ui, Color32, Frame, RichText, Vec2, Sense};
use crate::{InspectorApp, SettingsTab};

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("System Settings");
    ui.add_space(20.0);

    // Two-Column Vertical Tab Layout
    ui.columns(2, |columns| {
        // Left column (width adjusted for sidebar navigation)
        let left_ui = &mut columns[0];
        left_ui.set_max_width(180.0);
        left_ui.vertical(|ui| {
            ui.label(RichText::new("SECTIONS").small().strong().color(app.theme.text_secondary));
            ui.add_space(6.0);

            let options = [
                (SettingsTab::General, "⚙ GENERAL"),
                (SettingsTab::Audio, "🔊 AUDIO"),
                (SettingsTab::Midi, "🎹 MIDI"),
                (SettingsTab::Network, "🌐 NETWORK"),
                (SettingsTab::Calibration, "📐 CALIBRATION"),
            ];

            nullherz_ui_hal::widgets::render_segmented_control_vertical(
                ui,
                &app.theme,
                &mut app.active_settings_tab,
                &options,
                160.0,
            );

            ui.add_space(40.0);
            if ui.button(RichText::new("🛑 STOP ENGINE").strong().color(app.theme.danger)).clicked() {
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
}

fn render_general(app: &mut InspectorApp, ui: &mut Ui) {
    ui.strong("Global Transport");
    ui.add_space(4.0);
    Frame::none()
        .fill(app.theme.bg_surface)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Global BPM:");
                if ui.add(egui::DragValue::new(&mut app.global_bpm).clamp_range(40.0..=300.0)).changed() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetBpm(app.global_bpm)));
                }
            });
            ui.add_space(8.0);
            if ui.checkbox(&mut app.quantize_enabled, "Quantize Commands").changed() {
                 let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetSafeMode(app.quantize_enabled)));
            }
        });
}

fn render_audio(app: &mut InspectorApp, ui: &mut Ui) {
    ui.strong("Audio Engine Configuration");
    ui.add_space(4.0);
    Frame::none()
        .fill(app.theme.bg_surface)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("Select Audio Backend").color(app.theme.text_secondary));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("ALSA").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(nullherz_traits::AudioBackendType::Alsa)));
                }
                if ui.button("JACK").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(nullherz_traits::AudioBackendType::Jack)));
                }
                if ui.button("Pipewire").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(nullherz_traits::AudioBackendType::Pipewire)));
                }
                if ui.button("Threaded").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(nullherz_traits::AudioBackendType::Threaded)));
                }
            });

            ui.add_space(12.0);
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

    ui.add_space(15.0);
    ui.strong("Soundcard Wiring Test");
    ui.add_space(4.0);
    Frame::none()
        .fill(app.theme.bg_surface)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("Verify the physical outputs and routing of your audio interface by outputting a test signal.").color(app.theme.text_secondary));
            ui.add_space(10.0);

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
    ui.strong("MIDI Hardware & Mappings");
    ui.add_space(4.0);
    Frame::none()
        .fill(app.theme.bg_surface)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_md)
        .show(ui, |ui| {
            ui.label("Active Port Mappings:");
            ui.label("• Pioneer DDJ-400 (Attached)");
            ui.label("• Generic MIDI Keyboard (Attached)");
            ui.add_space(6.0);

            if ui.button("BIND DETECTED PORTS").clicked() {
                let ports = "Pioneer DDJ-400,Generic MIDI Keyboard";
                let mut buffer = [0u8; 128];
                let bytes = ports.as_bytes();
                let len = bytes.len().min(128);
                buffer[..len].copy_from_slice(&bytes[..len]);
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMidiPorts(buffer)));
            }

            ui.add_space(15.0);
            ui.label("Controller Profiles:");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let options = ["default", "pioneer_ddj400", "akai_mpk_mini"];
                for opt in options {
                    if ui.button(format!("Load {}", opt)).clicked() {
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
    ui.strong("Distributed Sidecar Discovery");
    ui.add_space(4.0);
    Frame::none()
        .fill(app.theme.bg_surface)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("P2P Cloud Sync and Remote DSP Nodes").color(app.theme.text_secondary));
            ui.add_space(12.0);

            ui.label("Remote Nodes Detected:");
            ui.add_space(6.0);

            let remote_nodes = [
                ("192.168.1.45 (Studio-PC-2)", true),
                ("192.168.1.12 (MacBook-Pro-DSP)", false),
            ];

            for (name, is_connected) in remote_nodes {
                Frame::none()
                    .fill(app.theme.bg_inset)
                    .rounding(app.theme.radius_md)
                    .stroke(app.theme.border_stroke)
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Status indicator dot
                            let dot_color = if is_connected { app.theme.success } else { app.theme.danger };
                            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                            ui.painter().circle_filled(dot_rect.center(), 4.0, dot_color);
                            ui.add_space(4.0);

                            ui.label(name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(if is_connected { "DISCONNECT" } else { "ATTACH" }).clicked() {
                                    println!("Toggling node connection...");
                                }
                            });
                        });
                    });
                ui.add_space(6.0);
            }
        });
}

fn render_calibration(app: &mut InspectorApp, ui: &mut Ui) {
    ui.strong("Hardware Latency Calibration");
    ui.add_space(4.0);
    Frame::none()
        .fill(app.theme.bg_surface)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_md)
        .show(ui, |ui| {
            ui.label("Measure Round-Trip Latency (RTL) to ensure sample-accurate alignment.");
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                if ui.button(RichText::new("● START CALIBRATION").color(app.theme.accent)).clicked() {
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

    ui.add_space(20.0);
    ui.strong("Distributed Clock Discipline (PTP/IEEE 1588)");
    ui.add_space(4.0);
    Frame::none()
        .fill(app.theme.bg_surface)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_md)
        .show(ui, |ui| {
            if let Some(t) = app.last_telemetry.lock().unwrap().as_ref() {
                ui.horizontal(|ui| {
                    ui.label("Sync Status:");
                    if t.clock_jitter_ns < 1000 {
                        ui.label(RichText::new("● LOCKED").color(app.theme.success));
                    } else {
                        ui.label(RichText::new("○ SEEKING").color(app.theme.warning));
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

    ui.add_space(20.0);
    Frame::none()
        .fill(app.theme.bg_surface)
        .rounding(app.theme.radius_md)
        .stroke(app.theme.border_stroke)
        .inner_margin(app.theme.space_md)
        .show(ui, |ui| {
            if ui.button(RichText::new("SAVE SYSTEM CONFIG").strong()).clicked() {
                 let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology));
                 // Trigger actual persistence in conductor
                 let ports = "Pioneer DDJ-400,Generic MIDI Keyboard".to_string();
                 let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMidiPorts({
                     let mut b = [0u8; 128];
                     let bytes = ports.as_bytes();
                     b[..bytes.len().min(128)].copy_from_slice(&bytes[..bytes.len().min(128)]);
                     b
                 })));
                 println!("System configuration persisted.");
            }
        });
}
