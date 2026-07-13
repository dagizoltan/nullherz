use egui::{Ui, Frame, RichText};
use crate::InspectorApp;

pub fn render_midi(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("MIDI Hardware & Mappings");
    ui.add_space(theme.space_xs);

    // Query actual MIDI ports dynamically using midir if enabled
    let mut actual_ports: Vec<String> = Vec::new();
    let mut midi_error: Option<String> = None;

    #[cfg(feature = "midi-backend")]
    {
        match midir::MidiInput::new("Nullherz Inspector MIDI Scan") {
            Ok(midi_in) => {
                let ports: Vec<midir::MidiInputPort> = midi_in.ports();
                for port in &ports {
                    if let Ok(name) = midi_in.port_name(port) {
                        actual_ports.push(name);
                    }
                }
            }
            Err(e) => {
                midi_error = Some(e.to_string());
            }
        }
    }

    #[cfg(not(feature = "midi-backend"))]
    {
        midi_error = Some("Midir backend is disabled. Build with default features to enable live MIDI.".to_string());
    }

    // Display appropriate live discovery status banner
    ui.horizontal(|ui| {
        if midi_error.is_none() {
            ui.label(RichText::new("✔ LIVE MIDI: Dynamic hot-plug port scanner is active via ALSA/Midir.").size(9.0).color(theme.success));
        } else {
            ui.label(RichText::new(format!("⚠ MIDI SCANNER WARNING: {} (Running in emulation fallback mode).", midi_error.as_ref().unwrap())).size(9.0).color(theme.warning));
        }
    });
    ui.add_space(theme.space_xs);

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label("Active Port Mappings:");
            ui.add_space(theme.space_xs);

            if actual_ports.is_empty() {
                // Emulation fallback mode
                ui.label(RichText::new("No physical MIDI controllers detected. Presenting mock controllers for emulation:").size(theme.type_caption).color(theme.text_secondary));
                ui.label("• Pioneer DDJ-400 (Attached - Emulated)");
                ui.label("• Generic MIDI Keyboard (Attached - Emulated)");
            } else {
                for name in &actual_ports {
                    ui.label(format!("• {} (Attached - Active)", name));
                }
            }
            ui.add_space(theme.space_sm);

            if ui.button("BIND DETECTED PORTS").clicked() {
                let ports = if actual_ports.is_empty() {
                    "Pioneer DDJ-400,Generic MIDI Keyboard".to_string()
                } else {
                    actual_ports.join(",")
                };
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
