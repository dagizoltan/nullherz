use egui::{Color32, RichText, Ui, Frame, ScrollArea, Layout, Align};
use crate::InspectorApp;
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, _telemetry: &Option<Telemetry>) {
    ui.heading("Global Modulation Matrix");
    ui.add_space(10.0);
    ui.label(RichText::new("Route macros to any engine parameter with custom scaling.").color(Color32::from_gray(120)));
    ui.add_space(20.0);

    ScrollArea::vertical().show(ui, |ui| {
        ui.set_width(ui.available_width());

        for macro_idx in 0..8 {
            Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(15.0).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("MACRO {:02}: {}", macro_idx + 1, app.macro_names[macro_idx])).strong().color(Color32::from_rgb(0, 255, 200)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("+ ADD MAPPING").clicked() {
                                // Logic to add mapping (placeholder)
                            }
                        });
                    });

                    ui.add_space(10.0);

                    // Table Header
                    ui.horizontal(|ui| {
                        ui.set_height(20.0);
                        ui.label(RichText::new("TARGET NODE").small().color(Color32::from_gray(80)));
                        ui.add_space(100.0);
                        ui.label(RichText::new("PARAM ID").small().color(Color32::from_gray(80)));
                        ui.add_space(60.0);
                        ui.label(RichText::new("SCALING").small().color(Color32::from_gray(80)));
                        ui.add_space(100.0);
                        ui.label(RichText::new("RAMP").small().color(Color32::from_gray(80)));
                    });

                    ui.painter().hline(ui.available_rect_before_wrap().x_range(), ui.next_widget_position().y, egui::Stroke::new(1.0, Color32::from_gray(30)));
                    ui.add_space(5.0);

                    // Mock mappings for UI demonstration
                    for i in 0..2 {
                         ui.horizontal(|ui| {
                            ui.label(format!("Node {}", (macro_idx * 4 + i)));
                            ui.add_space(120.0);
                            ui.label(format!("{}", i));
                            ui.add_space(100.0);
                            ui.label("1.00");
                            ui.add_space(120.0);
                            ui.label("128 ms");

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("🗑").on_hover_text("Remove Mapping").clicked() {
                                    let _ = app.command_sender.send(nullherz_traits::Command::RemoveModMapping {
                                        macro_id: macro_idx as u32,
                                        target_id: (macro_idx * 4 + i) as u64,
                                        param_id: i as u32,
                                    });
                                }
                            });
                        });
                    }
                });
            });
            ui.add_space(10.0);
        }
    });
}
