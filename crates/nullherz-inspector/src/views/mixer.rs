use egui::{Ui, Frame, Margin, Rounding, Stroke, RichText};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    ui.heading(RichText::new("System Mixer").size(theme.type_heading));
    ui.add_space(theme.space_md);

    ui.horizontal(|ui| {
        for i in 0..4 {
            let deck_color = crate::InspectorApp::deck_color(&theme, i);
            // Resolve this channel's REAL node ids from the telemetry node
            // map. Hardcoded indices go stale every time the bootstrap
            // layout changes (the old `10 + i` fader target now lands on
            // deck A's SEQUENCER, and the old `peak_levels[i]` meter read
            // deck A's strip for every channel).
            let deck = ['a', 'b', 'c', 'd'][i];
            let gain_node = app.topo.node_map.get(&format!("deck_{}_gain", deck)).copied();
            let meter_node = app
                .topo.node_map.get(&format!("deck_{}_isolator", deck))
                .or_else(|| app.topo.node_map.get(&format!("deck_{}_sampler", deck)))
                .copied();
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
                            let r_fader = widgets::render_fader(ui, &mut app.mixer.channel_faders[i], 0.0..=1.2, deck_color, 120.0, 30.0);
                            if r_fader.changed()
                                && let Some(gain_id) = gain_node {
                                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                        target_id: gain_id as u64,
                                        param_id: 0,
                                        value: app.mixer.channel_faders[i],
                                        ramp_duration_samples: 128,
                                    }));
                                }
                            if r_fader.drag_stopped() || r_fader.lost_focus() {
                                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CheckpointParameterEdit));
                            }

                            if let (Some(t), Some(node)) = (telemetry, meter_node) {
                                let level = t.peak_levels.get(node as usize).copied().unwrap_or(0.0);
                                widgets::render_vu_meter(ui, level, app.mixer.channel_peak_hold[i], deck_color, 120.0);
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
