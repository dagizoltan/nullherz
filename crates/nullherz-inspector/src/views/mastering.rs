use egui::{Color32, RichText, Ui, Frame, Layout, Align, Vec2};
use crate::InspectorApp;
use crate::widgets;
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, _telemetry: &Option<Telemetry>) {
    ui.heading("Precision Mastering Console");
    ui.add_space(20.0);

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(20.0, 0.0);

        // EQ MODULE
        Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(15.0).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("LINEAR PHASE EQ").color(Color32::from_rgb(0, 255, 200)).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.checkbox(&mut app.mastering_eq_enabled, "");
                    });
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if widgets::render_knob(ui, &mut app.mastering_eq_low, 0.0..=2.0, "LOW", Color32::from_rgb(0, 255, 200)).changed() {
                        let _ = app.command_sender.send(nullherz_traits::Command::SetParam { target_id: 19, param_id: 0, value: app.mastering_eq_low, ramp_duration_samples: 128 });
                    }
                    if widgets::render_knob(ui, &mut app.mastering_eq_mid, 0.0..=2.0, "MID", Color32::from_rgb(0, 255, 200)).changed() {
                        let _ = app.command_sender.send(nullherz_traits::Command::SetParam { target_id: 19, param_id: 1, value: app.mastering_eq_mid, ramp_duration_samples: 128 });
                    }
                    if widgets::render_knob(ui, &mut app.mastering_eq_high, 0.0..=2.0, "HIGH", Color32::from_rgb(0, 255, 200)).changed() {
                        let _ = app.command_sender.send(nullherz_traits::Command::SetParam { target_id: 19, param_id: 2, value: app.mastering_eq_high, ramp_duration_samples: 128 });
                    }
                });
            });
        });

        // COMPRESSOR MODULE
        Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(15.0).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("BUS COMPRESSOR").color(Color32::from_rgb(255, 180, 0)).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.checkbox(&mut app.mastering_comp_enabled, "");
                    });
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if widgets::render_knob(ui, &mut app.mastering_comp_threshold, 0.0..=1.0, "THR", Color32::from_rgb(255, 180, 0)).changed() {
                        let _ = app.command_sender.send(nullherz_traits::Command::SetParam { target_id: 20, param_id: 0, value: app.mastering_comp_threshold, ramp_duration_samples: 128 });
                    }
                    if widgets::render_knob(ui, &mut app.mastering_comp_ratio, 0.0..=1.0, "RATIO", Color32::from_rgb(255, 180, 0)).changed() {
                        let _ = app.command_sender.send(nullherz_traits::Command::SetParam { target_id: 20, param_id: 1, value: app.mastering_comp_ratio, ramp_duration_samples: 128 });
                    }
                    if widgets::render_knob(ui, &mut app.mastering_comp_attack, 0.0..=1.0, "ATTACK", Color32::from_rgb(255, 180, 0)).changed() {
                        let _ = app.command_sender.send(nullherz_traits::Command::SetParam { target_id: 20, param_id: 2, value: app.mastering_comp_attack, ramp_duration_samples: 128 });
                    }

                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("GR").size(8.0).color(Color32::from_gray(100)));
                        let gr = if app.mastering_comp_enabled { (app.mastering_comp_threshold * 0.5).sin().abs() } else { 0.0 };
                        widgets::render_vu_meter(ui, gr, gr, Color32::RED, 60.0);
                    });
                });
            });
        });

        // LIMITER MODULE
        Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(15.0).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("BRICKWALL LIMITER").color(Color32::from_rgb(255, 50, 50)).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.checkbox(&mut app.mastering_limiter_enabled, "");
                    });
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if widgets::render_knob(ui, &mut app.mastering_limiter_gain, 0.0..=1.5, "CEIL", Color32::from_rgb(255, 50, 50)).changed() {
                        let _ = app.command_sender.send(nullherz_traits::Command::SetParam { target_id: 21, param_id: 0, value: app.mastering_limiter_gain, ramp_duration_samples: 128 });
                    }
                    if widgets::render_knob(ui, &mut app.mastering_limiter_lookahead, 0.0..=1.0, "LOOK", Color32::from_rgb(255, 50, 50)).changed() {
                        let _ = app.command_sender.send(nullherz_traits::Command::SetParam { target_id: 21, param_id: 1, value: app.mastering_limiter_lookahead, ramp_duration_samples: 128 });
                    }
                });
            });
        });
    });

    ui.add_space(30.0);
    Frame::none().fill(Color32::from_rgb(10, 10, 12)).inner_margin(20.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("MASTER SIGNAL FLOW").color(Color32::from_gray(80)).size(10.0));
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            for (name, color) in [("IN", Color32::WHITE), ("EQ", Color32::from_rgb(0, 255, 200)), ("COMP", Color32::from_rgb(255, 180, 0)), ("LIMIT", Color32::from_rgb(255, 50, 50)), ("OUT", Color32::WHITE)] {
                ui.label(RichText::new(name).color(color).strong());
                if name != "OUT" {
                    ui.label(RichText::new(" ➔ ").color(Color32::from_gray(40)));
                }
            }
        });
    });
}
