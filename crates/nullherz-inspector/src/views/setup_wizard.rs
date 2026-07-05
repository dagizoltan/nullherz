use egui::{Ui, Color32, RichText};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("Alpha Setup Wizard");
    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label(RichText::new("Welcome to Nullherz Alpha. Please configure your hardware environment below.").color(Color32::from_gray(180)));
        ui.add_space(20.0);

        ui.strong("1. Audio Backend");
        ui.horizontal(|ui| {
            ui.label("Current Selection:");
            ui.label(RichText::new("ALSA (Optimized)").color(Color32::from_rgb(0, 255, 200)));
        });

        ui.horizontal(|ui| {
            if ui.button("ALSA").clicked() {
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(nullherz_traits::AudioBackendType::Alsa)));
            }
            if ui.button("JACK").clicked() {
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(nullherz_traits::AudioBackendType::Jack)));
            }
            if ui.button("Threaded").clicked() {
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(nullherz_traits::AudioBackendType::Threaded)));
            }
            if ui.button("Mock").clicked() {
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(nullherz_traits::AudioBackendType::Mock)));
            }
        });

        if ui.button("Scan for Backends").clicked() {
             // Bridge to nullherz-setup logic (mocked for UI)
             println!("Scanning for audio backends...");
        }
        ui.add_space(10.0);

        ui.strong("2. MIDI Hardware");
        ui.label("Detecting controllers...");
        ui.group(|ui| {
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
        });
        if ui.button("Refresh MIDI List").clicked() {
             println!("Refreshing MIDI device list...");
        }
        ui.add_space(10.0);

        ui.strong("3. Distributed DSP Discovery");
        ui.label("Searching for remote sidecar nodes on local network...");
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

        ui.add_space(10.0);
        ui.strong("4. Hardware Calibration");
        ui.horizontal(|ui| {
            if ui.button("MEASURE LATENCY (RTL)").clicked() {
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CalibrateLatency));
            }
            ui.label("Current RTL: 10.0ms (441 samples)");
        });

        ui.add_space(30.0);
        if ui.button(RichText::new("FINALIZE CONFIGURATION").strong().size(18.0)).clicked() {
            let config = nullherz_conductor::persistence::SystemConfig {
                audio_backend: "Alsa".to_string(), // In a real app we'd track current selection
                midi_ports: vec!["Pioneer DDJ-400".to_string(), "Generic MIDI Keyboard".to_string()],
                sample_rate: 44100,
                block_size: 256,
                calibration_samples: 441,
            };
            let json = serde_json::to_string_pretty(&config).unwrap_or_default();
            let _ = std::fs::write("system_config.json", json);

            println!("Configuration saved to system_config.json");
            app.active_view = crate::View::Console;
        }
    });
}
