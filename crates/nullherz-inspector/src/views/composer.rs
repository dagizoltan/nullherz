use egui::{Ui, RichText, Frame, Color32, ScrollArea, vec2};
use crate::{InspectorApp, widgets};

const PANEL_ROUNDING: f32 = 4.0;
const INNER_MARGIN: f32 = 8.0;

pub fn render(_app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("Composer - Performance Launcher");
    ui.add_space(INNER_MARGIN);

    ScrollArea::both().show(ui, |ui| {
        ui.horizontal_top(|ui| {
            // Tracks 1-8
            for t in 0..8 {
                ui.vertical(|ui| {
                    ui.set_width(100.0);

                    // TRACK HEADER
                    Frame::none().fill(Color32::from_gray(25)).rounding(PANEL_ROUNDING).inner_margin(INNER_MARGIN / 2.0).show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(format!("TRACK {}", t+1)).small().strong());
                            ui.horizontal(|ui| {
                                let _ = ui.button(RichText::new("S").small());
                                let _ = ui.button(RichText::new("M").small());
                            });
                            widgets::render_vu_meter(ui, 0.0, 0.0, Color32::from_rgb(0, 255, 100), 60.0);
                        });
                    });

                    ui.add_space(8.0);

                    // CLIPS
                    for s_idx in 0..8 {
                        let (rect, res) = ui.allocate_exact_size(vec2(100.0, 32.0), egui::Sense::click());
                        let color = if s_idx == 2 && t == 1 { Color32::from_rgb(0, 255, 100) } else { Color32::from_gray(20) };
                        ui.painter().rect_filled(rect, 2.0, color);
                        ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, Color32::from_gray(40)));

                        if res.clicked() { /* Trigger clip */ }
                    }

                    ui.add_space(10.0);
                    // VOLUME FADER
                    let mut vol = 0.8;
                    widgets::render_fader(ui, &mut vol, 0.0..=1.0, Color32::from_gray(100), 100.0, 15.0);
                });
                ui.add_space(8.0);
            }

            ui.separator();

            // MASTER / SCENE LAUNCHER
            ui.vertical(|ui| {
                ui.set_width(60.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("MASTER").small().strong());
                    ui.add_space(76.0); // Align with clip top
                    for _s in 0..8 {
                        if ui.add(egui::Button::new(RichText::new("▶").small()).min_size(vec2(50.0, 32.0))).clicked() {
                            // Launch scene
                        }
                        ui.add_space(4.0);
                    }
                });
            });
        });
    });
}
