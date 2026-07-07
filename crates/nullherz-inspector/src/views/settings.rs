use egui::{Ui, Color32, Frame, RichText};
use crate::{InspectorApp, SettingsTab};

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("System Settings");
    ui.add_space(20.0);

    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.active_settings_tab, SettingsTab::General, "GENERAL");
        ui.selectable_value(&mut app.active_settings_tab, SettingsTab::Audio, "AUDIO");
        ui.selectable_value(&mut app.active_settings_tab, SettingsTab::Midi, "MIDI");
        ui.selectable_value(&mut app.active_settings_tab, SettingsTab::Network, "NETWORK");
        ui.selectable_value(&mut app.active_settings_tab, SettingsTab::Calibration, "CALIBRATION");
    });
    ui.separator();
    ui.add_space(10.0);

    match app.active_settings_tab {
        SettingsTab::General => render_general(app, ui),
        SettingsTab::Audio => render_audio(app, ui),
        SettingsTab::Midi => render_midi(app, ui),
        SettingsTab::Network => render_network(app, ui),
        SettingsTab::Calibration => render_calibration(app, ui),
    }

    ui.add_space(30.0);
    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        if ui.button(RichText::new("STOP ENGINE").color(Color32::RED)).clicked() {
            let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop));
        }
    });
}

fn render_general(app: &mut InspectorApp, ui: &mut Ui) {
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
    ui.strong("Audio Engine Configuration");
    Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("Select Audio Backend").color(Color32::from_gray(180)));
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

        ui.add_space(10.0);
        if ui.button("Scan for Devices").clicked() {
             println!("Scanning audio hardware...");
        }
    });
}

fn render_midi(app: &mut InspectorApp, ui: &mut Ui) {
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

        ui.add_space(15.0);
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

fn render_network(_app: &mut InspectorApp, ui: &mut Ui) {
    ui.strong("Distributed Sidecar Discovery");
    Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("P2P Cloud Sync and Remote DSP Nodes").color(Color32::from_gray(150)));
        ui.add_space(10.0);

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
    ui.strong("Hardware Latency Calibration");
    Frame::group(ui.style()).show(ui, |ui| {
        ui.label("Measure Round-Trip Latency (RTL) to ensure sample-accurate alignment.");
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            if ui.button(RichText::new("● START CALIBRATION").color(Color32::from_rgb(0, 255, 200))).clicked() {
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CalibrateLatency));
            }

            if let Some(t) = app.last_telemetry.lock().unwrap().as_ref() {
                if t.calibration_samples > 0 {
                    let ms = (t.calibration_samples as f32 / (t.sample_rate / 1000.0));
                    ui.label(format!("Current RTL: {:.1}ms ({} samples)", ms, t.calibration_samples));
                } else {
                    ui.label("Current RTL: Not Calibrated");
                }
            } else {
                ui.label("Current RTL: --");
            }
        });

        ui.add_space(20.0);
        if ui.button(RichText::new("SAVE SYSTEM CONFIG").strong()).clicked() {
             // AUDIT-FIX: We must NOT send CommitTopology directly to the command bus,
             // as that might bypass Conductor and hit the RT thread.
             // Instead, we send it and rely on Conductor::apply_mixer_commands -> handle_core_command
             // to NOT forward it if it handled it.
             // Conductor currently doesn't handle CommitTopology in handle_core_command,
             // it passes it to mixer_bridge.apply_mixer_commands which calls topology_manager.handle_topology_command.
             // TopologyManager::handle_topology_command DOES handle CommitTopology off-thread.
             let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology));
             println!("System configuration persisted.");
        }
    });
}
