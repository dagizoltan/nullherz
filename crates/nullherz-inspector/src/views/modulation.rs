use egui::{Ui, Color32, RichText};
use crate::InspectorApp;
use audio_core::Telemetry;
use nullherz_traits::{Command, MixerCommand};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, _telemetry: &Option<Telemetry>) {
    ui.heading("Modulation Routing Matrix");
    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label(RichText::new("ANAWAVES TRAIT INHERITANCE").color(Color32::from_gray(100)));
        ui.add_space(5.0);
        ui.vertical(|ui| {
            ui.label("Global Spectral Window");
            let prev_shape = app.spectral_window_shape;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut app.spectral_window_shape, 0, "Hann");
                ui.selectable_value(&mut app.spectral_window_shape, 1, "Hamming");
                ui.selectable_value(&mut app.spectral_window_shape, 2, "Blackman");
                ui.selectable_value(&mut app.spectral_window_shape, 3, "Rectangular");
            });

            if app.spectral_window_shape != prev_shape {
                 let _ = app.command_sender.send(nullherz_traits::Command::Extension(nullherz_traits::OpaqueEnvelope {
                    domain_id: 0x53504543, // "SPEC"
                    target_id: 100, // Assuming spectral morph node
                    opcode: 0x01,
                    data: [app.spectral_window_shape as u8, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                }));
            }
        });
    });

    ui.add_space(20.0);

    // Matrix Header
    ui.horizontal(|ui| {
        ui.add_sized([120.0, 20.0], egui::Label::new("MACRO SOURCE"));
        for i in 0..8 {
            ui.add_sized([40.0, 20.0], egui::Label::new(format!("M{}", i + 1)));
        }
    });

    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Mock targets for demonstration
        let targets = [
            (150, 0, "Transfusion Bias"),
            (150, 1, "Rhythmic Bias"),
            (150, 2, "Artifact Bias"),
            (150, 3, "Spatial Bias"),
            (200, 0, "Master Filter"),
        ];

        for (node_id, param_id, name) in targets {
            ui.horizontal(|ui| {
                ui.add_sized([120.0, 25.0], egui::Label::new(name));

                for macro_id in 0..8 {
                    // Logic to check if mapping exists would go here.
                    // For now, toggle-able checkboxes to add/remove mappings.
                    let mut is_mapped = false;

                    if ui.add_sized([40.0, 25.0], egui::Checkbox::without_text(&mut is_mapped)).clicked() {
                        if is_mapped {
                            let _ = app.command_sender.send(Command::Mixer(MixerCommand::AddModMapping {
                                macro_id,
                                target_id: node_id,
                                param_id,
                                scaling: 1.0,
                                ramp_duration_samples: 0,
                            }));
                        } else {
                            let _ = app.command_sender.send(Command::Mixer(MixerCommand::RemoveModMapping {
                                macro_id,
                                target_id: node_id,
                                param_id,
                            }));
                        }
                    }
                }
            });
        }
    });

    ui.add_space(20.0);
    ui.label("Macro Control Bank");
    ui.horizontal(|ui| {
        for i in 0..8 {
            ui.vertical(|ui| {
                let prev_val = app.macros[i];
                crate::widgets::render_knob(ui, &mut app.macros[i], 0.0..=1.0, &format!("M{}", i+1), Color32::from_rgb(0, 255, 200));
                if app.macros[i] != prev_val {
                    let _ = app.command_sender.send(Command::Mixer(MixerCommand::SetMacro {
                        macro_id: i as u32,
                        value: app.macros[i],
                    }));
                }
            });
        }
    });
}
