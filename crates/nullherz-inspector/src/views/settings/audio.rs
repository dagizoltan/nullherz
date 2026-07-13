use egui::{Ui, Frame, RichText};
use crate::InspectorApp;
use nullherz_traits::AudioBackendType;

pub fn render_audio(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("Audio Engine Configuration");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("Select Audio Backend").color(theme.text_secondary));
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                let backends = [
                    (AudioBackendType::Alsa, "ALSA"),
                    (AudioBackendType::Jack, "JACK"),
                    (AudioBackendType::Pipewire, "Pipewire"),
                    (AudioBackendType::Threaded, "Threaded"),
                ];

                for (backend, label) in backends {
                    let is_active = app.active_backend == backend;
                    let mut btn = egui::Button::new(label);
                    if is_active {
                        btn = btn.fill(theme.accent.linear_multiply(0.12))
                                 .stroke(egui::Stroke::new(1.0, theme.accent));
                    }
                    if ui.add(btn).clicked() {
                        app.active_backend = backend;
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(backend)));
                    }
                }
            });

            ui.add_space(theme.space_md);
            ui.horizontal(|ui| {
                ui.label("Device:");
                egui::ComboBox::from_id_source("audio_device_select")
                    .selected_text(&app.selected_audio_device)
                    .show_ui(ui, |ui| {
                        for dev in &app.audio_devices {
                            ui.selectable_value(&mut app.selected_audio_device, dev.clone(), dev);
                        }
                    });

                if ui.button("Scan for Devices").clicked() {
                     // Trigger re-enumeration in backend (noop for now as orchestrator does it every tick)
                }
            });
        });

    ui.add_space(theme.space_md);
    ui.strong("Soundcard Wiring Test");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("Verify the physical outputs and routing of your audio interface by outputting a test signal.").color(theme.text_secondary));
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                if ui.button("▶ PLAY TEST PREVIEW (TRACK A)").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::Preview { sample_id: 1 }));
                }
                if ui.button("▶ PLAY TEST PREVIEW (TRACK B)").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::Preview { sample_id: 2 }));
                }
                if ui.button("⏹ STOP TEST").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::StopNode {
                        node_idx: nullherz_traits::NodeConventions::PREVIEW,
                    }));
                }
            });
        });
}
