use egui::{Ui, Frame, RichText, Stroke, Color32, Margin, Rounding, Button};
use crate::InspectorApp;

pub fn render_midi(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.horizontal(|ui| {
        ui.heading(RichText::new("MIDI Hardware & Mappings").strong().color(theme.text_primary));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(format!("{} Refresh Devices", egui_phosphor::regular::ARROWS_COUNTER_CLOCKWISE)).clicked() {
                app.poll_auto_midi_discovery(0.0);
            }
        });
    });
    ui.add_space(theme.space_xs);

    // --- 1. AUTO-DISCOVERY & HARDWARE STATUS CARD ---
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut app.settings.auto_midi_discovery, "Enable Auto MIDI Device Discovery");
                ui.add_space(theme.space_md);

                if app.settings.auto_midi_discovery {
                    Frame::none()
                        .fill(theme.success.linear_multiply(0.12))
                        .stroke(Stroke::new(1.0_f32, theme.success))
                        .rounding(Rounding::same(theme.radius_sm))
                        .inner_margin(Margin::symmetric(6.0, 2.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new(format!("{} AUTO-DISCOVERY ACTIVE", egui_phosphor::regular::CHECK_CIRCLE)).size(10.0).strong().color(theme.success));
                        });
                } else {
                    Frame::none()
                        .fill(theme.warning.linear_multiply(0.12))
                        .stroke(Stroke::new(1.0_f32, theme.warning))
                        .rounding(Rounding::same(theme.radius_sm))
                        .inner_margin(Margin::symmetric(6.0, 2.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new(format!("{} MANUAL BIND MODE", egui_phosphor::regular::HANDS_CLAPPING)).size(10.0).strong().color(theme.warning));
                        });
                }
            });

            ui.add_space(theme.space_xs);
            ui.label(
                RichText::new("Nullherz automatically scans and hot-plugs connected physical MIDI controllers (ALSA Seq / CoreMIDI) and binds them dynamically to the engine.")
                    .size(theme.type_caption)
                    .color(theme.text_secondary),
            );

            ui.add_space(theme.space_sm);
            ui.separator();
            ui.add_space(theme.space_xs);

            ui.label(RichText::new("Discovered MIDI Input Devices:").strong().size(theme.type_caption).color(theme.text_primary));
            ui.add_space(theme.space_xs);

            let actual_ports = if app.settings.discovered_midi_ports.is_empty() {
                // Emulated fallback mode when no physical devices attached
                vec![
                    "Pioneer DDJ-400 (Emulated)".to_string(),
                    "Generic MIDI Keyboard (Emulated)".to_string(),
                ]
            } else {
                app.settings.discovered_midi_ports.clone()
            };

            ui.horizontal_wrapped(|ui| {
                for port_name in &actual_ports {
                    Frame::none()
                        .fill(theme.bg_inset)
                        .rounding(theme.radius_sm)
                        .stroke(Stroke::new(1.0_f32, theme.border))
                        .inner_margin(Margin::symmetric(theme.space_sm, theme.space_xs))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(egui_phosphor::regular::PLUG).color(theme.accent).size(12.0));
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(port_name).strong().size(11.0).color(theme.text_primary));
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("Status: Connected & Auto-Bound").size(9.0).color(theme.success));
                                        ui.label(RichText::new("• Channel: All").size(9.0).color(theme.text_secondary));
                                    });
                                });
                            });
                        });
                    ui.add_space(theme.space_xs);
                }
            });

            ui.add_space(theme.space_sm);

            if ui.button(format!("{} Force Re-Bind Detected Ports", egui_phosphor::regular::PLUGS_CONNECTED)).clicked() {
                let ports = actual_ports.join(",");
                let mut buffer = [0u8; 128];
                let bytes = ports.as_bytes();
                let len = bytes.len().min(128);
                buffer[..len].copy_from_slice(&bytes[..len]);
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMidiPorts(buffer)));
            }
        });

    ui.add_space(theme.space_md);

    // --- 2. CONTROLLER PROFILES & MAPPING SCHEME VISUALIZER ---
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Controller Profiles & Mapping Visualizer").strong().size(theme.type_body).color(theme.text_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("Active Profile: {}", app.settings.active_midi_profile)).strong().color(theme.accent));
                });
            });
            ui.add_space(theme.space_xs);
            ui.label(
                RichText::new("Select a pre-configured controller mapping profile or custom JSON mapping from library/mappings/.")
                    .size(theme.type_caption)
                    .color(theme.text_secondary),
            );
            ui.add_space(theme.space_sm);

            let profiles = [
                ("default", "Default Studio"),
                ("keyboard", "Keyboard Controller"),
                ("pioneer_ddj400", "Pioneer DDJ-400"),
                ("pioneer_ddj_flx4", "Pioneer DDJ-FLX4"),
                ("native_instruments_traktor_s2", "Traktor Kontrol S2"),
                ("akai_mpk_mini", "Akai MPK Mini"),
                ("novation_launchkey_mini", "Launchkey Mini"),
                ("arturia_minilab_3", "Arturia MiniLab 3"),
                ("numark_mixtrack_pro_fx", "Mixtrack Pro FX"),
                ("hercules_djcontrol_inpulse_300", "Hercules InPulse 300"),
            ];

            ui.horizontal_wrapped(|ui| {
                for (id, label) in profiles {
                    let is_active = app.settings.active_midi_profile == id;
                    let mut btn = Button::new(RichText::new(label).size(10.0).strong());
                    if is_active {
                        btn = btn.fill(theme.accent.linear_multiply(0.18))
                                 .stroke(Stroke::new(1.0_f32, theme.accent));
                    } else {
                        btn = btn.fill(theme.bg_inset);
                    }

                    if ui.add(btn).clicked() {
                        app.settings.active_midi_profile = id.to_string();
                        let mut buffer = [0u8; 32];
                        let bytes = id.as_bytes();
                        let len = bytes.len().min(32);
                        buffer[..len].copy_from_slice(&bytes[..len]);
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::LoadMidiMap(buffer)));
                    }
                }
            });

            ui.add_space(theme.space_md);
            ui.separator();
            ui.add_space(theme.space_xs);

            // Mapping Inspector Grid for active profile
            ui.label(RichText::new("Active Control Mapping Layout Scheme:").strong().size(theme.type_caption).color(theme.text_primary));
            ui.add_space(theme.space_xs);

            ui.vertical(|ui| {
                Frame::none()
                    .fill(theme.bg_inset)
                    .rounding(theme.radius_sm)
                    .inner_margin(theme.space_sm)
                    .show(ui, |ui| {
                        ui.label(RichText::new("DECK & TRANSPORT").strong().size(10.0).color(theme.accent));
                        ui.add_space(2.0);
                        ui.label(RichText::new("• Play / Pause: Note 0x0B / CC 11").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Cue Point Set: Note 0x0C / CC 12").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Tempo Pitch Fader: CC 16 (0..127)").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Jog Wheel Nudge: CC 33 / CC 34").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Slip Mode Toggle: Note 0x20").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Loop In / Out / Exit: Notes 0x18..0x1A").size(9.0).color(theme.text_secondary));
                    });

                ui.add_space(theme.space_xs);

                Frame::none()
                    .fill(theme.bg_inset)
                    .rounding(theme.radius_sm)
                    .inner_margin(theme.space_sm)
                    .show(ui, |ui| {
                        ui.label(RichText::new("MIXER & EQ CANALS").strong().size(10.0).color(theme.accent));
                        ui.add_space(2.0);
                        ui.label(RichText::new("• Channel Faders 1..4: CC 20..23").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Crossfader: CC 31 (Deck A ↔ Deck B)").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Master Volume Fader: CC 7").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• HI / MID / LOW EQ: CC 40..51").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Trim Gain Stage: CC 52..55").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Filter LP/HP Sweep: CC 60..63").size(9.0).color(theme.text_secondary));
                    });

                ui.add_space(theme.space_xs);

                Frame::none()
                    .fill(theme.bg_inset)
                    .rounding(theme.radius_sm)
                    .inner_margin(theme.space_sm)
                    .show(ui, |ui| {
                        ui.label(RichText::new("PADS & PERFORMANCE").strong().size(10.0).color(theme.accent));
                        ui.add_space(2.0);
                        ui.label(RichText::new("• Hot Cues 1..8: Notes 0x30..0x37").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Sampler Pads 1..16: Notes 0x24..0x33").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• FX Rack Toggle: Notes 0x40..0x43").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Beat Jump ±4: Notes 0x38..0x39").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Stems Demix Solo: CC 70..73").size(9.0).color(theme.text_secondary));
                        ui.label(RichText::new("• Visuals SPD/TMP: CC 80..83").size(9.0).color(theme.text_secondary));
                    });
            });
        });

    ui.add_space(theme.space_md);

    // --- 3. QWERTY VIRTUAL MIDI KEYBOARD CONTROLLER ---
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("QWERTY Computer Keyboard as Virtual MIDI Device");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut app.settings.qwerty_midi_enabled, "Enable Keyboard Controller");
                });
            });
            ui.add_space(theme.space_xs);
            ui.label(RichText::new("Play synth and sampler notes directly using computer keyboard keys (DAW layout: Z-M base, Q-I +1 octave)").size(theme.type_caption).color(theme.text_secondary));
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                ui.label("Octave Transpose:");
                if ui.button("-").clicked() {
                    app.settings.qwerty_octave = (app.settings.qwerty_octave - 1).max(-2);
                }
                ui.label(RichText::new(format!("{:+}", app.settings.qwerty_octave)).strong().color(theme.accent));
                if ui.button("+").clicked() {
                    app.settings.qwerty_octave = (app.settings.qwerty_octave + 1).min(2);
                }
                let base_note = (60i16 + (app.settings.qwerty_octave as i16) * 12).clamp(0, 127);
                ui.label(RichText::new(format!("(Base Note: C{} / MIDI {})", (base_note / 12) as i16 - 1, base_note)).color(theme.text_secondary));
            });

            ui.add_space(theme.space_sm);
            ui.label(RichText::new("Virtual Keybed Visualizer (Live Key Pressing Feedback):").strong().size(10.0).color(theme.text_primary));
            ui.add_space(theme.space_xs);

            // Lower Octave Keys Row (Z-M base)
            ui.label(RichText::new("LOWER OCTAVE (Z..M):").size(9.0).strong().color(theme.accent));
            ui.horizontal(|ui| {
                let lower_keys = [
                    (egui::Key::Z, "Z", "C"),
                    (egui::Key::S, "S", "C#"),
                    (egui::Key::X, "X", "D"),
                    (egui::Key::D, "D", "D#"),
                    (egui::Key::C, "C", "E"),
                    (egui::Key::V, "V", "F"),
                    (egui::Key::G, "G", "F#"),
                    (egui::Key::B, "B", "G"),
                    (egui::Key::H, "H", "G#"),
                    (egui::Key::N, "N", "A"),
                    (egui::Key::J, "J", "A#"),
                    (egui::Key::M, "M", "B"),
                    (egui::Key::Comma, ",", "C+1"),
                    (egui::Key::L, "L", "C#+1"),
                    (egui::Key::Period, ".", "D+1"),
                ];

                for (key, k_label, n_label) in lower_keys {
                    let is_held = app.settings.qwerty_held_keys.contains(&key);
                    let is_sharp = n_label.contains('#');
                    let bg = if is_held {
                        theme.accent
                    } else if is_sharp {
                        Color32::from_rgb(25, 28, 38)
                    } else {
                        theme.bg_inset
                    };
                    let text_color = if is_held { theme.bg_canvas } else if is_sharp { theme.accent } else { theme.text_primary };

                    Frame::none()
                        .fill(bg)
                        .rounding(theme.radius_sm)
                        .stroke(Stroke::new(1.0_f32, if is_held { theme.accent } else { theme.border }))
                        .inner_margin(Margin::symmetric(8.0, 6.0))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new(k_label).size(11.0).strong().color(text_color));
                                ui.label(RichText::new(n_label).size(8.0).color(if is_held { theme.bg_canvas } else { theme.text_secondary }));
                            });
                        });
                }
            });

            ui.add_space(theme.space_xs);

            // Upper Octave Keys Row (Q-I +1 Octave)
            ui.label(RichText::new("UPPER OCTAVE (Q..I [+1 Octave]):").size(9.0).strong().color(theme.accent));
            ui.horizontal(|ui| {
                let upper_keys = [
                    (egui::Key::Q, "Q", "C+1"),
                    (egui::Key::Num2, "2", "C#+1"),
                    (egui::Key::W, "W", "D+1"),
                    (egui::Key::Num3, "3", "D#+1"),
                    (egui::Key::E, "E", "E+1"),
                    (egui::Key::R, "R", "F+1"),
                    (egui::Key::Num5, "5", "F#+1"),
                    (egui::Key::T, "T", "G+1"),
                    (egui::Key::Num6, "6", "G#+1"),
                    (egui::Key::Y, "Y", "A+1"),
                    (egui::Key::Num7, "7", "A#+1"),
                    (egui::Key::U, "U", "B+1"),
                    (egui::Key::I, "I", "C+2"),
                ];

                for (key, k_label, n_label) in upper_keys {
                    let is_held = app.settings.qwerty_held_keys.contains(&key);
                    let is_sharp = n_label.contains('#');
                    let bg = if is_held {
                        theme.accent
                    } else if is_sharp {
                        Color32::from_rgb(25, 28, 38)
                    } else {
                        theme.bg_inset
                    };
                    let text_color = if is_held { theme.bg_canvas } else if is_sharp { theme.accent } else { theme.text_primary };

                    Frame::none()
                        .fill(bg)
                        .rounding(theme.radius_sm)
                        .stroke(Stroke::new(1.0_f32, if is_held { theme.accent } else { theme.border }))
                        .inner_margin(Margin::symmetric(8.0, 6.0))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new(k_label).size(11.0).strong().color(text_color));
                                ui.label(RichText::new(n_label).size(8.0).color(if is_held { theme.bg_canvas } else { theme.text_secondary }));
                            });
                        });
                }
            });
        });

    ui.add_space(theme.space_md);

    // --- 4. LIVE MIDI STREAM MONITOR & EVENT LOG ---
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Live MIDI Stream Monitor Log");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear Log").clicked() {
                        app.settings.recent_midi_events.clear();
                    }
                });
            });
            ui.add_space(theme.space_xs);

            if app.settings.recent_midi_events.is_empty() {
                ui.label(
                    RichText::new("No MIDI events recorded yet. Connect a physical MIDI controller or press QWERTY piano keys...")
                        .color(theme.text_secondary)
                        .size(theme.type_caption),
                );
            } else {
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for ev in app.settings.recent_midi_events.iter().rev() {
                            let (status_label, pill_bg, pill_fg) = match ev.status & 0xF0 {
                                0x90 => if ev.data2 > 0 { ("Note On", theme.success.linear_multiply(0.18), theme.success) } else { ("Note Off", theme.bg_inset, theme.text_secondary) },
                                0x80 => ("Note Off", theme.bg_inset, theme.text_secondary),
                                0xB0 => ("CC Control", theme.accent.linear_multiply(0.18), theme.accent),
                                0xE0 => ("Pitch Bend", theme.warning.linear_multiply(0.18), theme.warning),
                                _ => ("MIDI Event", theme.bg_inset, theme.text_primary),
                            };

                            let channel = (ev.status & 0x0F) + 1;

                            ui.horizontal(|ui| {
                                Frame::none()
                                    .fill(pill_bg)
                                    .stroke(Stroke::new(1.0_f32, pill_fg))
                                    .rounding(Rounding::same(theme.radius_sm))
                                    .inner_margin(Margin::symmetric(4.0, 1.0))
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(status_label).size(9.0).strong().color(pill_fg));
                                    });

                                ui.label(
                                    RichText::new(format!(
                                        "Ch {} | Data1 (Note/CC): {} | Data2 (Val/Vel): {}",
                                        channel, ev.data1, ev.data2
                                    ))
                                    .size(10.0)
                                    .color(theme.text_primary),
                                );
                            });
                        }
                    });
            }
        });
}
