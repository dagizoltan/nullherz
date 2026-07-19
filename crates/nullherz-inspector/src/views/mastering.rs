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

    let render_left_panel = |ui: &mut Ui| {
        Frame::none()
            .fill(theme.bg_surface)
            .rounding(Rounding::same(theme.radius_md))
            .inner_margin(Margin::same(15.0))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("3-BAND EQ").strong().size(theme.type_body).color(theme.text_secondary));
                    ui.add_space(theme.space_sm);
                    // NOT WIRED — deliberately disabled. The old knobs sent
                    // SetParam to hardcoded node 19 (today: deck B's cue-send
                    // gain), and there is no master tone stage to bind them
                    // to: the master chain's BIQUAD takes raw coefficients
                    // (b0/b1/b2), which LOW/MID/HIGH gains are not. Enable
                    // this only once a real master isolator/EQ node exists.
                    ui.add_enabled_ui(false, |ui| {
                        ui.horizontal(|ui| {
                            let mut zero = 1.0f32;
                            widgets::render_knob(ui, &mut zero, 0.0..=2.0, "LOW", theme.text_disabled);
                            let mut zero = 1.0f32;
                            widgets::render_knob(ui, &mut zero, 0.0..=2.0, "MID", theme.text_disabled);
                            let mut zero = 1.0f32;
                            widgets::render_knob(ui, &mut zero, 0.0..=2.0, "HIGH", theme.text_disabled);
                        });
                    });
                    ui.add_space(theme.space_xs);
                    ui.label(
                        RichText::new("Not wired — no master EQ stage in the graph yet.")
                            .size(theme.type_caption)
                            .color(theme.text_disabled),
                    );
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
