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

    // The MASTERING_EQ node in the master chain (RBJ shelves/peak; params
    // 0/1/2 = LOW/MID/HIGH linear gain, 1.0 = flat). Resolved by name like
    // every other master node; knobs stay disabled if the graph lacks it.
    let eq_node = app.get_node_id("master_eq");

    let mut render_left_panel = |ui: &mut Ui| {
        Frame::none()
            .fill(theme.bg_surface)
            .rounding(Rounding::same(theme.radius_md))
            .inner_margin(Margin::same(15.0))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("3-BAND EQ").strong().size(theme.type_body).color(theme.text_secondary));
                    ui.add_space(theme.space_sm);
                    ui.add_enabled_ui(eq_node.is_some(), |ui| {
                        ui.horizontal(|ui| {
                            let r_low = widgets::render_knob(ui, &mut app.mixer.mastering_eq_low, 0.0..=2.0, "LOW", theme.accent);
                            let r_mid = widgets::render_knob(ui, &mut app.mixer.mastering_eq_mid, 0.0..=2.0, "MID", theme.accent);
                            let r_high = widgets::render_knob(ui, &mut app.mixer.mastering_eq_high, 0.0..=2.0, "HIGH", theme.accent);

                            if let Some(node) = eq_node {
                                // ~46 ms ramp at 44.1k: click-free on the
                                // master without feeling laggy under a knob.
                                const RAMP: u32 = 2048;
                                let bands = [
                                    (&r_low, 0u32, app.mixer.mastering_eq_low),
                                    (&r_mid, 1u32, app.mixer.mastering_eq_mid),
                                    (&r_high, 2u32, app.mixer.mastering_eq_high),
                                ];
                                for (resp, param_id, value) in bands {
                                    if resp.changed() {
                                        let _ = app.command_sender.send(nullherz_traits::Command::Mixer(
                                            nullherz_traits::MixerCommand::SetParam {
                                                target_id: node as u64,
                                                param_id,
                                                value,
                                                ramp_duration_samples: RAMP,
                                            },
                                        ));
                                    }
                                }
                                if r_low.drag_stopped() || r_low.lost_focus()
                                    || r_mid.drag_stopped() || r_mid.lost_focus()
                                    || r_high.drag_stopped() || r_high.lost_focus()
                                {
                                    let _ = app.command_sender.send(nullherz_traits::Command::Core(
                                        nullherz_traits::CoreCommand::CheckpointParameterEdit,
                                    ));
                                }
                            }
                        });
                    });
                    if eq_node.is_none() {
                        ui.add_space(theme.space_xs);
                        ui.label(
                            RichText::new("master_eq node not found in the graph.")
                                .size(theme.type_caption)
                                .color(theme.text_disabled),
                        );
                    }
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
