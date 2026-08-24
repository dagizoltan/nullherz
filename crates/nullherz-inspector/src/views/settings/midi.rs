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
            ui.horizontal_wrapped(|ui| {
                let options = [
                    "default", "keyboard", "pioneer_ddj400", "pioneer_ddj_flx4",
                    "native_instruments_traktor_s2", "akai_mpk_mini",
                    "novation_launchkey_mini", "arturia_minilab_3",
                    "numark_mixtrack_pro_fx", "hercules_djcontrol_inpulse_300"
                ];
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

            ui.add_space(theme.space_md);
            ui.label("MIDI Learn:");
            ui.add_space(theme.space_xs);
            ui.horizontal_wrapped(|ui| {
                let targets = [
                    ("Deck A Gain", nullherz_traits::MidiTarget::NamedParam { node_name: "deck_a_gain".to_string(), param_id: 0 }),
                    ("Deck B Gain", nullherz_traits::MidiTarget::NamedParam { node_name: "deck_b_gain".to_string(), param_id: 0 }),
                    ("Crossfader", nullherz_traits::MidiTarget::NamedParam { node_name: "master_crossfader".to_string(), param_id: 0 }),
                    ("Master Volume", nullherz_traits::MidiTarget::NamedParam { node_name: "master_limiter".to_string(), param_id: 0 }),
                    ("Deck A Play", nullherz_traits::MidiTarget::Command(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::PlayDeck { deck_id: 'A' }))),
                    ("Deck B Play", nullherz_traits::MidiTarget::Command(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::PlayDeck { deck_id: 'B' }))),
                ];

                for (label, target) in targets {
                    if ui.button(format!("Learn {}", label)).clicked() {
                        send_start_midi_learn(app, &target);
                    }
                }

                if ui.button("Save Custom Map").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SaveCustomMidiMap));
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
            ui.label("Interactive 2-Octave Piano Keyboard:");
            ui.add_space(theme.space_xs);

            render_piano_keyboard_widget(ui, app);
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

pub fn send_start_midi_learn(app: &InspectorApp, target: &nullherz_traits::MidiTarget) {
    let mut target_json = [0u8; 128];
    if let Ok(json) = serde_json::to_string(target) {
        let bytes = json.as_bytes();
        let len = bytes.len().min(128);
        target_json[..len].copy_from_slice(&bytes[..len]);
    }
    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::StartMidiLearn { target_json }));
}

pub fn render_piano_keyboard_widget(ui: &mut Ui, app: &mut InspectorApp) {
    let theme = app.theme;
    let base_note = (60i16 + (app.settings.qwerty_octave as i16) * 12).clamp(0, 127) as u8;

    let keys = [
        (0u8, "C", false, Some(egui::Key::Z), "Z"),
        (1u8, "C#", true, Some(egui::Key::S), "S"),
        (2u8, "D", false, Some(egui::Key::X), "X"),
        (3u8, "D#", true, Some(egui::Key::D), "D"),
        (4u8, "E", false, Some(egui::Key::C), "C"),
        (5u8, "F", false, Some(egui::Key::V), "V"),
        (6u8, "F#", true, Some(egui::Key::G), "G"),
        (7u8, "G", false, Some(egui::Key::B), "B"),
        (8u8, "G#", true, Some(egui::Key::H), "H"),
        (9u8, "A", false, Some(egui::Key::N), "N"),
        (10u8, "A#", true, Some(egui::Key::J), "J"),
        (11u8, "B", false, Some(egui::Key::M), "M"),
        (12u8, "C", false, Some(egui::Key::Comma), ","),
        (13u8, "C#", true, Some(egui::Key::L), "L"),
        (14u8, "D", false, Some(egui::Key::Period), "."),
        (15u8, "D#", true, Some(egui::Key::Num3), "3"),
        (16u8, "E", false, Some(egui::Key::E), "E"),
        (17u8, "F", false, Some(egui::Key::R), "R"),
        (18u8, "F#", true, Some(egui::Key::Num5), "5"),
        (19u8, "G", false, Some(egui::Key::T), "T"),
        (20u8, "G#", true, Some(egui::Key::Num6), "6"),
        (21u8, "A", false, Some(egui::Key::Y), "Y"),
        (22u8, "A#", true, Some(egui::Key::Num7), "7"),
        (23u8, "B", false, Some(egui::Key::U), "U"),
    ];

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (semi, note_name, is_black, key_opt, key_str) in keys {
            let note = (base_note + semi).min(127);
            let is_held = key_opt.map_or(false, |k| app.settings.qwerty_held_keys.contains(&k));

            let bg = if is_held {
                theme.accent
            } else if is_black {
                theme.bg_canvas
            } else {
                theme.bg_surface
            };

            let text_color = if is_held {
                theme.bg_canvas
            } else if is_black {
                theme.text_secondary
            } else {
                theme.text_primary
            };

            let key_width = if is_black { 20.0 } else { 26.0 };
            let key_height = if is_black { 44.0 } else { 60.0 };

            let btn = egui::Button::new(
                RichText::new(format!("{}\n{}", note_name, key_str))
                    .size(9.0)
                    .color(text_color)
                    .strong(),
            )
            .min_size(egui::vec2(key_width, key_height))
            .fill(bg);

            let resp = ui.add(btn);

            if resp.clicked() {
                let event = nullherz_traits::MidiEvent {
                    timestamp_samples: 0,
                    status: 0x90,
                    data1: note,
                    data2: 100,
                    _pad: 0,
                };
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::InjectMidi(event)));
                app.settings.recent_midi_events.push_back(event);
            }
        }
    });
}
