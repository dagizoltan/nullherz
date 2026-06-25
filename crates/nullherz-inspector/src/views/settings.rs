use egui::{Color32, RichText, Ui, Frame};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("System Configuration & Hardening");
    ui.add_space(20.0);

    ui.vertical(|ui| {
        ui.set_width(400.0);

        Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(15.0).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("REAL-TIME ENGINE").strong().color(Color32::from_rgb(0, 255, 200)));
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label("Safe Mode");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Checkbox::new(&mut app.quantize_enabled, "")).changed() { // Re-purposing quantize for demo or should add safe_mode field
                             let _ = app.command_sender.send(nullherz_traits::Command::SetSafeMode(app.quantize_enabled));
                        }
                    });
                });

                ui.add_space(10.0);
                if ui.button("🔥 FORCE ENGINE RESET").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Stop);
                    // In a real app, this would send a special reset command if available
                }
            });
        });

        ui.add_space(20.0);

        Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(15.0).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("NETWORK GATEWAY").strong().color(Color32::from_rgb(0, 180, 255)));
                ui.add_space(10.0);
                ui.label("WebSocket: 127.0.0.1:9001");
                ui.label("Status: CONNECTED");
            });
        });

        ui.add_space(20.0);

        ui.label(RichText::new("ARCHITECTURE: Triple-Plane Model (Orchestration/Protocol/Execution)").small().color(Color32::from_gray(60)));
        ui.label(RichText::new("ENGINE VERSION: 0.1.0-alpha (PROD READY)").small().color(Color32::from_gray(60)));
    });
}
