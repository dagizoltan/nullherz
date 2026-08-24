use egui::{Ui, Frame, RichText};
use crate::InspectorApp;

pub fn render_midi(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("MIDI Hardware & Mappings");
    ui.add_space(theme.space_xs);

    // Query actual MIDI ports dynamically using midir if enabled
    #[allow(unused_mut)]
    let mut actual_ports: Vec<String> = Vec::new();

    let midi_error: Option<String> = {
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
                    None
                }
                Err(e) => Some(e.to_string()),
            }
        }
        #[cfg(not(feature = "midi-backend"))]
        {
            Some("Midir backend is disabled. Build with default features to enable live MIDI.".to_string())
        }
    };

    // Display appropriate live discovery status banner
    ui.horizontal(|ui| {
        if let Some(ref err) = midi_error {
            ui.label(RichText::new(format!("⚠ MIDI SCANNER WARNING: {} (Running in emulation fallback mode).", err)).size(9.0).color(theme.warning));
        } else {
            ui.label(RichText::new("✔ LIVE MIDI: Dynamic hot-plug port scanner is active via ALSA/Midir.").size(9.0).color(theme.success));
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
                let options = ["default", "keyboard", "pioneer_ddj400", "akai_mpk_mini"];
                for opt in options {
                    let is_active = app.settings.active_midi_profile == opt;
                    let mut btn = egui::Button::new(format!("Load {}", opt));
                    if is_active {
                        btn = btn.fill(theme.accent.linear_multiply(0.12))
                                 .stroke(egui::Stroke::new(1.0, theme.accent));
                    }
                    if ui.add(btn).clicked() {
                        app.settings.active_midi_profile = opt.to_string();
                        let mut buffer = [0u8; 32];
                        let bytes = opt.as_bytes();
                        let len = bytes.len().min(32);
                        buffer[..len].copy_from_slice(&bytes[..len]);
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::LoadMidiMap(buffer)));
                    }
                }
            });
        });

    ui.add_space(theme.space_md);

    // QWERTY Virtual MIDI Keyboard Controller Section
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.strong("QWERTY Computer Keyboard as MIDI Device");
            ui.add_space(theme.space_xs);
            ui.label(RichText::new("Play synth/sampler notes directly using computer keyboard keys (DAW layout: Z-M & Q-I)").size(theme.type_caption).color(theme.text_secondary));
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                ui.checkbox(&mut app.settings.qwerty_midi_enabled, "Enable Keyboard MIDI Controller Input");
                ui.add_space(theme.space_md);
                ui.label("Octave:");
                if ui.button("-").clicked() {
                    app.settings.qwerty_octave = (app.settings.qwerty_octave - 1).max(-2);
                }
                ui.label(format!("{:+}", app.settings.qwerty_octave));
                if ui.button("+").clicked() {
                    app.settings.qwerty_octave = (app.settings.qwerty_octave + 1).min(2);
                }
                let base_note = (60i16 + (app.settings.qwerty_octave as i16) * 12).clamp(0, 127);
                ui.label(RichText::new(format!("(Base: C{})", (base_note / 12) as i16 - 1)).color(theme.text_secondary));
            });

            ui.add_space(theme.space_sm);
            ui.label("Virtual Key Visualizer:");
            ui.add_space(theme.space_xs);

            ui.horizontal(|ui| {
                let white_keys = [
                    (egui::Key::Z, "Z (C)"),
                    (egui::Key::X, "X (D)"),
                    (egui::Key::C, "C (E)"),
                    (egui::Key::V, "V (F)"),
                    (egui::Key::B, "B (G)"),
                    (egui::Key::N, "N (A)"),
                    (egui::Key::M, "M (B)"),
                    (egui::Key::Comma, ", (C+1)"),
                    (egui::Key::Period, ". (D+1)"),
                ];

                for (key, label) in white_keys {
                    let is_held = app.settings.qwerty_held_keys.contains(&key);
                    let bg = if is_held { theme.accent } else { theme.bg_inset };
                    let text_color = if is_held { theme.bg_canvas } else { theme.text_primary };

                    Frame::none()
                        .fill(bg)
                        .rounding(theme.radius_sm)
                        .stroke(theme.border_stroke)
                        .inner_margin(egui::Margin::symmetric(theme.space_xs, theme.space_xs))
                        .show(ui, |ui| {
                            ui.label(RichText::new(label).size(10.0).color(text_color).strong());
                        });
                }
            });
        });

    ui.add_space(theme.space_md);

    // Live MIDI Event Log
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.strong("Live MIDI Event Log");
            ui.add_space(theme.space_xs);

            if app.settings.recent_midi_events.is_empty() {
                ui.label(RichText::new("No MIDI events recorded yet. Press QWERTY piano keys or move MIDI controls...").color(theme.text_secondary).size(theme.type_caption));
            } else {
                egui::ScrollArea::vertical()
                    .max_height(120.0)
                    .show(ui, |ui| {
                        for ev in app.settings.recent_midi_events.iter().rev() {
                            let status_type = match ev.status & 0xF0 {
                                0x90 => if ev.data2 > 0 { "Note On" } else { "Note Off" },
                                0x80 => "Note Off",
                                0xB0 => "CC",
                                0xE0 => "Pitch Bend",
                                _ => "MIDI Event",
                            };
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("[{}] Status: 0x{:02X} | Data1: {} | Data2: {}", status_type, ev.status, ev.data1, ev.data2)).size(10.0).color(theme.accent));
                            });
                        }
                    });
            }
        });
}
