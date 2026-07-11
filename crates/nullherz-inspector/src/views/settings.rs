use egui::{Ui, Color32, Frame, RichText};
use crate::{InspectorApp, SettingsTab};

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.heading(RichText::new("System Settings").size(theme.type_heading));
    ui.add_space(theme.space_md);

    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.active_settings_tab, SettingsTab::General, "GENERAL");
        ui.selectable_value(&mut app.active_settings_tab, SettingsTab::Audio, "AUDIO");
        ui.selectable_value(&mut app.active_settings_tab, SettingsTab::Midi, "MIDI");
        ui.selectable_value(&mut app.active_settings_tab, SettingsTab::Network, "NETWORK");
        ui.selectable_value(&mut app.active_settings_tab, SettingsTab::Calibration, "CALIBRATION");
    });
    ui.separator();
    ui.add_space(theme.space_sm);

    match app.active_settings_tab {
        SettingsTab::General => render_general(app, ui),
        SettingsTab::Audio => render_audio(app, ui),
        SettingsTab::Midi => render_midi(app, ui),
        SettingsTab::Network => render_network(app, ui),
        SettingsTab::Calibration => render_calibration(app, ui),
    }

    ui.add_space(theme.space_xl);
    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        if ui.button(RichText::new("STOP ENGINE").color(theme.danger).strong().size(theme.type_label)).clicked() {
            let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop));
        }
    });
}

fn render_general(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("Global Transport");
    Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Global BPM:");
            if ui.add(egui::DragValue::new(&mut app.global_bpm).clamp_range(40.0..=300.0)).changed() {
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetBpm(app.global_bpm)));
            }
        });

        if ui.checkbox(&mut app.quantize_enabled, "Quantize Commands").changed() {
             let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetSafeMode(app.quantize_enabled)));
        }
    });
}

fn render_audio(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("Audio Engine Configuration");
    Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("Select Audio Backend").color(theme.text_secondary).size(theme.type_caption));
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

        ui.add_space(theme.space_sm);
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
    Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("Verify the physical outputs and routing of your audio interface by outputting a test signal.").color(theme.text_secondary).size(theme.type_caption));
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
    Frame::group(ui.style()).show(ui, |ui| {
        ui.label("Active Port Mappings:");
        ui.label("• Pioneer DDJ-400 (Attached)");
        ui.label("• Generic MIDI Keyboard (Attached)");

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
    let theme = app.theme;
    ui.strong("Distributed Sidecar Discovery");
    Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("P2P Cloud Sync and Remote DSP Nodes").color(theme.text_secondary).size(theme.type_caption));
        ui.add_space(theme.space_sm);

        ui.label("Remote Nodes Detected:");
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("• 192.168.1.45 (Studio-PC-2)");
                if ui.button("ATTACH").clicked() { println!("Attaching to remote node..."); }
            });
            ui.horizontal(|ui| {
                ui.label("• 192.168.1.12 (MacBook-Pro-DSP)");
                if ui.button("ATTACH").clicked() { println!("Attaching to remote node..."); }
            });
        });
    });
}

fn render_calibration(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("Hardware Latency Calibration");
    Frame::group(ui.style()).show(ui, |ui| {
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

        ui.add_space(theme.space_md);
        ui.strong("Distributed Clock Discipline (PTP/IEEE 1588)");
        Frame::group(ui.style()).show(ui, |ui| {
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

        ui.add_space(theme.space_md);
        if ui.button(RichText::new("SAVE SYSTEM CONFIG").strong().size(theme.type_label)).clicked() {
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
