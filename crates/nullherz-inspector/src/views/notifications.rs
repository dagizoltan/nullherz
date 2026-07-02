use egui::{Ui, ScrollArea, Color32, RichText};
use crate::InspectorApp;

pub fn render(_app: &InspectorApp, ui: &mut Ui) {
    ui.heading(RichText::new("AI ANALYSIS").strong().color(Color32::from_rgb(0, 255, 200)));
    ui.add_space(10.0);

    ScrollArea::vertical().show(ui, |ui| {
        ui.vertical(|ui| {
            // DNA Analysis Results Section
            ui.group(|ui| {
                ui.label(RichText::new("LATEST DNA TAGGING").strong());
                ui.separator();
                ui.label(RichText::new("• Sample 'Breakbeat_01' analyzed.").color(Color32::from_gray(150)));
                ui.label(RichText::new("  Spectral Tilt: 0.42 (Bright)").color(Color32::from_gray(120)));
                ui.label(RichText::new("  Syncopation: 0.85 (High)").color(Color32::from_gray(120)));
            });

            ui.add_space(10.0);

            // AI Suggestions Section
            ui.group(|ui| {
                ui.label(RichText::new("TRANSFUSION SUGGESTIONS").strong());
                ui.separator();
                ui.vertical(|ui| {
                    ui.label("Personality Match Found!");
                    ui.label(RichText::new("Deck 1 and 'Atmospheric_Pad_04' share high harmonicity. Try a 50% Transfusion?").size(10.0));
                    if ui.button("EXECUTE").clicked() {
                        // In the future, this would send a Command::Topology swap
                    }
                });
            });

            ui.add_space(10.0);

            // System Logs Section
            ui.group(|ui| {
                ui.label(RichText::new("SYSTEM EVENTS").strong());
                ui.separator();
                ui.label("• Sidecar #1: Performance stable.");
                ui.label("• AnalysisWorker: Registry updated.");
                ui.label(RichText::new("• Warning: X-RUN detected in block 1042").color(Color32::KHAKI));
            });
        });
    });
}
