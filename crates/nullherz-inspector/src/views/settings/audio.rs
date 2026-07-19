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
                    let is_active = app.settings.active_backend == backend;
                    let mut btn = egui::Button::new(label);
                    if is_active {
                        btn = btn.fill(theme.accent.linear_multiply(0.12))
                                 .stroke(egui::Stroke::new(1.0, theme.accent));
                    }
                    if ui.add(btn).clicked() {
                        app.settings.active_backend = backend;
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(backend)));
                    }
                }
            });

            ui.add_space(theme.space_md);
            ui.horizontal(|ui| {
                ui.label("Devices:");
                // DISPLAY ONLY — there is no device-selection command in the
                // protocol yet; the backend opens its default device. The old
                // dropdown let you "select" a device that nothing consumed,
                // and the "Scan" button was an admitted no-op (enumeration
                // already refreshes every tick via telemetry).
                ui.add_enabled_ui(false, |ui| {
                    egui::ComboBox::from_id_source("audio_device_select")
                        .selected_text(
                            app.settings.audio_devices.first().map(String::as_str).unwrap_or("(default)"),
                        )
                        .show_ui(ui, |_ui| {});
                });
                ui.label(
                    RichText::new("detected — output uses the backend default")
                        .size(theme.type_caption)
                        .color(theme.text_disabled),
                );
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
