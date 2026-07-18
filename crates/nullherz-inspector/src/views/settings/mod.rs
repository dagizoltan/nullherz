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
    ui.heading("System Settings");
    ui.add_space(theme.space_md);

    // Two-Column Vertical Tab Layout
    ui.columns(2, |columns| {
        // Left column (width adjusted for sidebar navigation)
        let left_ui = &mut columns[0];
        left_ui.set_max_width(180.0);
        left_ui.vertical(|ui| {
            ui.label(RichText::new("SECTIONS").small().strong().color(theme.text_secondary));
            ui.add_space(theme.space_xs);

            let options = [
                (SettingsTab::General, format!("{} GENERAL", egui_phosphor::regular::GEAR)),
                (SettingsTab::Audio, format!("{} AUDIO", egui_phosphor::regular::SPEAKER_HIGH)),
                (SettingsTab::Midi, format!("{} MIDI", egui_phosphor::regular::PIANO_KEYS)),
                (SettingsTab::Network, format!("{} NETWORK", egui_phosphor::regular::GLOBE)),
                (SettingsTab::Calibration, format!("{} CALIBRATION", egui_phosphor::regular::RULER)),
                (SettingsTab::Preferences, format!("{} PREFERENCES", egui_phosphor::regular::WRENCH)),
            ];

            let options_str: Vec<(SettingsTab, &str)> = options.iter().map(|(tab, label)| (*tab, label.as_str())).collect();

            nullherz_ui_hal::widgets::render_segmented_control_vertical(
                ui,
                &theme,
                &mut app.settings.active_settings_tab,
                &options_str,
                160.0,
            );

            ui.add_space(theme.space_lg);
            if ui.button(RichText::new(format!("{} STOP ENGINE", egui_phosphor::regular::STOP_CIRCLE)).strong().color(theme.danger)).clicked() {
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop));
            }
        });

        // Right column (scrollable settings pane)
        let right_ui = &mut columns[1];
        egui::ScrollArea::vertical().id_source("settings_pane_scroll").show(right_ui, |ui| {
            ui.set_min_width(380.0);
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

    ui.add_space(theme.space_lg);
    ui.separator();
    ui.add_space(theme.space_md);

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
}
