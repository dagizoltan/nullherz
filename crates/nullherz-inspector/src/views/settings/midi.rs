use egui::{Ui, Frame, RichText};
use crate::InspectorApp;

pub fn render_midi(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("MIDI Hardware & Mappings");
    ui.add_space(theme.space_xs);

    // Mock/planned disclosure for live MIDI device enumeration
    ui.horizontal(|ui| {
        ui.label(RichText::new("ℹ NOTE: MIDI Device enumeration is currently running in Mock/Simulated mode. Actual RT MIDI port mapping auto-discovery is not yet fully exposed by the midi_mapper.rs backend.").size(9.0).color(theme.text_secondary));
    });
    ui.add_space(theme.space_xs);

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label("Active Port Mappings:");
            ui.label("• Pioneer DDJ-400 (Attached)");
            ui.label("• Generic MIDI Keyboard (Attached)");
            ui.add_space(theme.space_sm);

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
