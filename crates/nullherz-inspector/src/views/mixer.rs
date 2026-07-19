use egui::{Ui, Frame, Margin, Rounding, Stroke, RichText, ScrollArea};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;

/// Fixed strip width: every card is the same size regardless of window
/// width. (The old layout used `vertical_centered` inside the horizontal
/// row, which expands to the FULL remaining width — the first card
/// ballooned and pushed the rest off the right edge.)
const STRIP_W: f32 = 96.0;
const FADER_H: f32 = 140.0;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    ui.heading(RichText::new("System Mixer").size(theme.type_heading));
    ui.add_space(theme.space_md);

    // Horizontal scroll instead of silent overflow on narrow windows.
    ScrollArea::horizontal().id_source("sys_mixer_scroll").show(ui, |ui| {
        ui.horizontal_top(|ui| {
            for i in 0..4 {
                render_channel_strip(app, ui, i, telemetry);
                ui.add_space(theme.space_sm);
            }
            ui.add_space(theme.space_md);
            render_master_strip(app, ui, telemetry);
        });
    });
}

fn render_channel_strip(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let deck_color = crate::InspectorApp::deck_color(&theme, i);
    let deck = ['a', 'b', 'c', 'd'][i];

    // Resolve this channel's REAL node ids from the telemetry node map.
    // Hardcoded indices go stale every time the bootstrap layout changes.
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
            ui.set_width(STRIP_W);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 40.0).max(0.0) / 2.0);
                    ui.label(RichText::new(format!("CH {}", (b'A' + i as u8) as char)).strong().size(theme.type_body).color(deck_color));
                });
                ui.add_space(theme.space_sm);

                // Fader & VU meter side-by-side
                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 24.0 - 8.0 - theme.space_sm).max(0.0) / 2.0);
                    let r_fader = widgets::render_fader(ui, &mut app.mixer.channel_faders[i], 0.0..=1.2, deck_color, FADER_H, 30.0);
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

                    ui.add_space(theme.space_sm);
                    if let (Some(t), Some(node)) = (telemetry, meter_node) {
                        let level = t.peak_levels.get(node as usize).copied().unwrap_or(0.0);
                        widgets::render_vu_meter(ui, level, app.mixer.channel_peak_hold[i], deck_color, FADER_H);
                    } else {
                        widgets::render_vu_meter(ui, 0.0, 0.0, theme.text_disabled, FADER_H);
                    }
                });

                ui.add_space(theme.space_xs);
                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 34.0).max(0.0) / 2.0);
                    ui.label(
                        RichText::new(format!("{:+.1} dB", 20.0 * app.mixer.channel_faders[i].max(1e-3).log10()))
                            .monospace()
                            .size(theme.type_caption)
                            .color(theme.text_secondary),
                    );
                });
            });
        });
}

fn render_master_strip(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let accent = theme.accent;

    // Master level is applied per side: the SUMMING nodes' gain (param 0).
    let sum_l = app.topo.node_map.get("master_sum_l").copied();
    let sum_r = app.topo.node_map.get("master_sum_r").copied();

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .inner_margin(Margin::same(theme.space_md))
        .stroke(Stroke::new(1.0, accent.gamma_multiply(0.6)))
        .show(ui, |ui| {
            ui.set_width(STRIP_W);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 52.0).max(0.0) / 2.0);
                    ui.label(RichText::new("MASTER").strong().size(theme.type_body).color(accent));
                });
                ui.add_space(theme.space_sm);

                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 24.0 - 20.0 - theme.space_sm).max(0.0) / 2.0);
                    let r_fader = widgets::render_fader(ui, &mut app.mixer.master_gain, 0.0..=1.2, accent, FADER_H, 30.0);
                    if r_fader.changed() {
                        for node in [sum_l, sum_r].into_iter().flatten() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: node as u64,
                                param_id: 0,
                                value: app.mixer.master_gain,
                                ramp_duration_samples: 128,
                            }));
                        }
                    }
                    if r_fader.drag_stopped() || r_fader.lost_focus() {
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CheckpointParameterEdit));
                    }

                    ui.add_space(theme.space_sm);
                    // Stereo pair: damped master peaks are bound to
                    // master_sum_l/r in the update loop.
                    let _ = telemetry;
                    widgets::render_vu_meter(ui, app.viz.damped_master_peaks[0], app.mixer.master_peak_hold, accent, FADER_H);
                    ui.add_space(2.0);
                    widgets::render_vu_meter(ui, app.viz.damped_master_peaks[1], app.mixer.master_peak_hold, accent, FADER_H);
                });

                ui.add_space(theme.space_xs);
                ui.horizontal(|ui| {
                    ui.add_space((STRIP_W - 34.0).max(0.0) / 2.0);
                    ui.label(
                        RichText::new(format!("{:+.1} dB", 20.0 * app.mixer.master_gain.max(1e-3).log10()))
                            .monospace()
                            .size(theme.type_caption)
                            .color(theme.text_secondary),
                    );
                });
            });
        });
}
