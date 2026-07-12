use egui::{Ui, RichText, Frame};
use crate::InspectorApp;
use audio_core::Telemetry;
use nullherz_traits::{Command, MixerCommand};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, _telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    ui.heading(RichText::new("Modulation Routing Matrix").size(theme.type_heading));
    ui.add_space(theme.space_sm);

    // Card 1: FFT Windowing Configuration
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("ANAWAVES TRAIT INHERITANCE").color(theme.text_secondary).size(theme.type_caption));
            ui.add_space(theme.space_xs);
            ui.vertical(|ui| {
                ui.label("Global Spectral Window");
                ui.add_space(theme.space_xs);

                let prev_shape = app.spectral_window_shape;
                let options = [
                    (0, "HANN"),
                    (1, "HAMMING"),
                    (2, "BLACKMAN"),
                    (3, "RECTANGULAR"),
                ];

                nullherz_ui_hal::widgets::render_segmented_control(
                    ui,
                    &theme,
                    &mut app.spectral_window_shape,
                    &options,
                );

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

    ui.add_space(theme.space_md);

    // Matrix Header
    ui.horizontal(|ui| {
        ui.add_sized([140.0, 20.0], egui::Label::new(RichText::new("MODULATION TARGET").size(theme.type_caption).strong()));
        for i in 0..8 {
            ui.add_sized([45.0, 20.0], egui::Label::new(RichText::new(format!("MACRO {}", i + 1)).size(theme.type_caption).color(theme.accent)));
        }
    });

    ui.separator();

    egui::ScrollArea::vertical().id_source("mod_matrix_scroll").show(ui, |ui| {
        // Dynamic targets from graph topology
        for (idx, node) in app.graph.nodes.iter().enumerate() {
            let node_id = idx as u64;
            // High-visibility grouping per node inside structured Card Frame
            Frame::none()
                .fill(theme.bg_surface)
                .rounding(theme.radius_md)
                .stroke(theme.border_stroke)
                .inner_margin(theme.space_md)
                .show(ui, |ui| {
                    ui.label(RichText::new(&node.name).strong().color(theme.text_primary).size(theme.type_body));
                    ui.add_space(theme.space_xs);

                    for param_id in 0..4 {
                        let name = format!("Param {}", param_id);
                        ui.horizontal(|ui| {
                            ui.add_sized([120.0, 25.0], egui::Label::new(RichText::new(name).size(theme.type_caption).color(theme.text_secondary)));

                            for macro_id in 0..8 {
                                // Hardened: Mini-knob for scaling control
                                let mut scaling = 0.0; // In a real app we'd fetch this from the modulation matrix state
                                ui.vertical(|ui| {
                                    ui.set_max_width(45.0);
                                    if crate::widgets::render_knob(ui, &mut scaling, -1.0..=1.0, "", theme.text_disabled).changed() {
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
            ui.add_space(theme.space_xs);
        }
    });

    ui.add_space(theme.space_md);
    ui.label("Macro Control Bank");
    ui.add_space(theme.space_xs);

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for i in 0..8 {
                    ui.vertical(|ui| {
                        let prev_val = app.macros[i];
                        crate::widgets::render_knob(ui, &mut app.macros[i], 0.0..=1.0, &format!("M{}", i+1), theme.accent);
                        if app.macros[i] != prev_val {
                            let _ = app.command_sender.send(Command::Mixer(MixerCommand::SetMacro {
                                macro_id: i as u32,
                                value: app.macros[i],
                            }));
                        }
                    });
                }
            });
        });
}
