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
        ui.add_sized([140.0, 20.0], egui::Label::new(RichText::new("MODULATION TARGET").small().strong()));
        for i in 0..8 {
            ui.add_sized([45.0, 20.0], egui::Label::new(RichText::new(format!("MACRO {}", i + 1)).small().color(Color32::from_rgb(0, 255, 200))));
        }
    });

    ui.separator();

    egui::ScrollArea::vertical().id_source("mod_matrix_scroll").show(ui, |ui| {
        // Dynamic targets from graph topology
        for (idx, node) in app.graph.nodes.iter().enumerate() {
            let node_id = idx as u64;
            // High-visibility grouping per node
            ui.group(|ui| {
                ui.label(RichText::new(&node.name).strong().color(Color32::WHITE));
                ui.add_space(4.0);

                for param_id in 0..4 {
                    let name = format!("Param {}", param_id);
                    ui.horizontal(|ui| {
                        ui.add_sized([120.0, 25.0], egui::Label::new(RichText::new(name).small().color(Color32::from_gray(180))));

                        for macro_id in 0..8 {
                            // Hardened: Mini-knob for scaling control
                            let mut scaling = 0.0; // In a real app we'd fetch this from the modulation matrix state
                            ui.vertical(|ui| {
                                ui.set_max_width(45.0);
                                if crate::widgets::render_knob(ui, &mut scaling, -1.0..=1.0, "", Color32::from_gray(120)).changed() {
                                     let _ = app.command_sender.send(Command::Mixer(MixerCommand::AddModMapping {
                                        macro_id: macro_id as u32,
                                        target_id: node_id,
                                        param_id,
                                        scaling,
                                        ramp_duration_samples: 0,
                                    }));
                                }
                            });
                        }
                    });
                }
            });
            ui.add_space(8.0);
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
