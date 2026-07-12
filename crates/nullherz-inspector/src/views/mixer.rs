use egui::{Ui, Frame, Margin, Rounding, Stroke, RichText, Color32};
use crate::{InspectorApp, widgets};
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    ui.heading(RichText::new("System Mixer").size(theme.type_heading));
    ui.add_space(theme.space_md);

    ui.horizontal(|ui| {
        for i in 0..4 {
            let deck_color = theme.deck_colors[i];
            Frame::none()
                .fill(theme.bg_surface)
                .rounding(Rounding::same(theme.radius_md))
                .inner_margin(Margin::same(theme.space_md))
                .stroke(Stroke::new(1.0, theme.border))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(format!("CH {}", i + 1)).strong().size(theme.type_body).color(deck_color));
                        ui.add_space(theme.space_sm);

                        // Fader & VU meter side-by-side
                        ui.horizontal(|ui| {
                            if widgets::render_fader(ui, &mut app.channel_faders[i], 0.0..=1.2, deck_color, 120.0, 30.0).changed() {
                                let target_id = (10 + i) as u64; // Example mapping
                                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id,
                                    param_id: 0,
                                    value: app.channel_faders[i],
                                    ramp_duration_samples: 128,
                                }));
                            }

                            if let Some(t) = telemetry {
                                let level = t.peak_levels[i];
                                widgets::render_vu_meter(ui, level, app.channel_peak_hold[i], deck_color, 120.0);
                            } else {
                                widgets::render_vu_meter(ui, 0.0, 0.0, theme.text_disabled, 120.0);
                            }
                        });
                    });
                });
            ui.add_space(theme.space_md);
        }
    });
}
