use egui::{Ui, Frame, RichText, Rounding, Margin};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    ui.heading(RichText::new("Precision Mastering").size(theme.type_heading));
    ui.add_space(theme.space_md);

    let available_w = ui.available_width();
    let is_wide = available_w > 650.0;

    let mut render_left_panel = |ui: &mut Ui| {
        Frame::none()
            .fill(theme.bg_surface)
            .rounding(Rounding::same(theme.radius_md))
            .inner_margin(Margin::same(15.0))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut app.mastering_eq_enabled, "3-BAND EQ");
                    });
                    ui.add_space(theme.space_sm);
                    ui.horizontal(|ui| {
                        if widgets::render_knob(ui, &mut app.mastering_eq_low, 0.0..=2.0, "LOW", theme.accent).changed() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id: 19, param_id: 0, value: app.mastering_eq_low, ramp_duration_samples: 128 }));
                        }
                        if widgets::render_knob(ui, &mut app.mastering_eq_mid, 0.0..=2.0, "MID", theme.accent).changed() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id: 19, param_id: 1, value: app.mastering_eq_mid, ramp_duration_samples: 128 }));
                        }
                        if widgets::render_knob(ui, &mut app.mastering_eq_high, 0.0..=2.0, "HIGH", theme.accent).changed() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id: 19, param_id: 2, value: app.mastering_eq_high, ramp_duration_samples: 128 }));
                        }
                    });
                });
            });
    };

    let render_right_panel = |ui: &mut Ui| {
        ui.vertical(|ui| {
            ui.strong("Final Stage Analysis");
            ui.add_space(theme.space_sm);
            if let Some(t) = telemetry {
                let layout_wide = ui.available_width() > 500.0;
                let show_viz = |ui: &mut Ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Phase/Goniometer").size(theme.type_caption).color(theme.text_secondary));
                        widgets::render_goniometer(ui, &t.goniometer_pts, 200.0, theme.accent);
                    });
                    if layout_wide {
                        ui.add_space(theme.space_lg);
                    } else {
                        ui.add_space(theme.space_sm);
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Master Spectrum").size(theme.type_caption).color(theme.text_secondary));
                        widgets::render_spectrum_analyzer(ui, &t.spectrum, theme.accent, 120.0);
                    });
                };
                if layout_wide {
                    ui.horizontal(|ui| show_viz(ui));
                } else {
                    ui.vertical(|ui| show_viz(ui));
                }
            } else {
                ui.label(RichText::new("No active telemetry...").size(theme.type_body).color(theme.text_disabled));
            }
        });
    };

    if is_wide {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(220.0);
                render_left_panel(ui);
            });
            ui.add_space(theme.space_md);
            render_right_panel(ui);
        });
    } else {
        ui.vertical(|ui| {
            render_left_panel(ui);
            ui.add_space(theme.space_md);
            render_right_panel(ui);
        });
    }
}
