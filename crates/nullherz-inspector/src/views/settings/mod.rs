use egui::{Ui, RichText};
use crate::{InspectorApp, SettingsTab};

pub mod general;
pub mod preferences;
pub mod audio;
pub mod midi;
pub mod network;
pub mod calibration;

pub use general::render_general;
pub use preferences::render_preferences;
pub use audio::render_audio;
pub use midi::render_midi;
pub use network::render_network;
pub use calibration::render_calibration;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    ui.vertical(|ui| {
        // TOP HORIZONTAL TAB BAR
        egui::Frame::none()
            .fill(theme.bg_surface)
            .rounding(theme.radius_md)
            .stroke(theme.border_stroke)
            .inner_margin(theme.space_md)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let options = [
                        (SettingsTab::General, format!("{} GENERAL", egui_phosphor::regular::GEAR)),
                        (SettingsTab::Audio, format!("{} AUDIO", egui_phosphor::regular::SPEAKER_HIGH)),
                        (SettingsTab::Midi, format!("{} MIDI", egui_phosphor::regular::PIANO_KEYS)),
                        (SettingsTab::Network, format!("{} NETWORK", egui_phosphor::regular::GLOBE)),
                        (SettingsTab::Calibration, format!("{} CALIBRATION", egui_phosphor::regular::RULER)),
                        (SettingsTab::Preferences, format!("{} PREFERENCES", egui_phosphor::regular::SLIDERS)),
                    ];

                    for (tab, label) in options {
                        let is_sel = app.settings.active_settings_tab == tab;
                        let btn = if is_sel {
                            egui::Button::new(RichText::new(&label).strong().size(theme.type_caption).color(theme.accent))
                                .fill(theme.accent.linear_multiply(0.18))
                                .stroke(egui::Stroke::new(1.0_f32, theme.accent))
                        } else {
                            egui::Button::new(RichText::new(&label).size(theme.type_caption).color(theme.text_primary))
                                .fill(theme.bg_inset)
                        };

                        if ui.add_sized([130.0, 28.0], btn).clicked() {
                            app.settings.active_settings_tab = tab;
                        }
                        ui.add_space(theme.space_xs);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([120.0, 28.0], egui::Button::new(RichText::new(format!("{} STOP ENGINE", egui_phosphor::regular::STOP_CIRCLE)).strong().size(theme.type_caption).color(theme.danger)).fill(theme.danger.linear_multiply(0.1))).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop));
                        }
                    });
                });
            });

        ui.add_space(theme.space_md);

        // VERTICAL SCROLLABLE SETTINGS PANE
        egui::ScrollArea::vertical()
            .id_source("settings_pane_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    match app.settings.active_settings_tab {
                        SettingsTab::General => render_general(app, ui),
                        SettingsTab::Audio => render_audio(app, ui),
                        SettingsTab::Midi => render_midi(app, ui),
                        SettingsTab::Network => render_network(app, ui),
                        SettingsTab::Calibration => render_calibration(app, ui),
                        SettingsTab::Preferences => render_preferences(app, ui),
                    }
                });
            });

        ui.add_space(theme.space_md);
        ui.separator();
        ui.add_space(theme.space_sm);

        // PERSISTENT FOOTER: "SAVE SYSTEM CONFIG" with transient feedback
        ui.horizontal(|ui| {
            let save_btn = egui::Button::new(RichText::new(format!("{} SAVE SYSTEM CONFIG", egui_phosphor::regular::FLOPPY_DISK)).strong().size(theme.type_label))
                .fill(theme.accent.linear_multiply(0.15));

            if ui.add_sized([200.0, 32.0], save_btn).clicked() {
                 let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology));
                 // Trigger actual persistence in conductor
                 let ports = "Pioneer DDJ-400,Generic MIDI Keyboard".to_string();
                 let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMidiPorts({
                     let mut b = [0u8; 128];
                     let bytes = ports.as_bytes();
                     b[..bytes.len().min(128)].copy_from_slice(&bytes[..bytes.len().min(128)]);
                     b
                 })));
                 app.settings.config_saved_time = Some(ui.input(|i| i.time));
            }

            if let Some(saved_t) = app.settings.config_saved_time {
                let current_t = ui.input(|i| i.time);
                if current_t - saved_t < 1.5 {
                    let text = if app.settings.autosave_triggered.is_some() {
                        format!("Autosaved {}", egui_phosphor::regular::CHECK)
                    } else {
                        format!("Saved {}", egui_phosphor::regular::CHECK)
                    };
                    ui.label(RichText::new(text).strong().color(theme.success));
                    ui.ctx().request_repaint(); // Keep repainting while banner is active
                } else {
                    app.settings.config_saved_time = None;
                    app.settings.autosave_triggered = None;
                }
            }
        });
    });
}
