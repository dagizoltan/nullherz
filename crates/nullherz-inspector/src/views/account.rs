use egui::{Ui, RichText, Color32};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("User Account");
    ui.add_space(20.0);

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("👤").size(32.0));
            ui.vertical(|ui| {
                ui.label(RichText::new("Local Producer").strong().size(18.0));
                ui.label("Identity: Node-7742 (Mastering Grade)");
            });
        });
    });

    ui.add_space(20.0);
    ui.label(RichText::new("SOUNDDNA IDENTITY PROFILE").strong());
    ui.group(|ui| {
        let tracks = app.cached_library.len();
        ui.label(format!("Genetic Material: {} samples", tracks));

        // Mock genetic traits based on cached library
        let mut avg_tilt = 0.0;
        if !app.cached_library.is_empty() {
            for t in &app.cached_library {
                avg_tilt += t.metadata.dna.spectral.tilt;
            }
            avg_tilt /= app.cached_library.len() as f32;
        }

        ui.label(format!("Dominant Trait: {}", if avg_tilt > 0.0 { "High-Frequency Clarity" } else { "Sub-Heavy Warmth" }));

        let progress = (tracks as f32 / 100.0).clamp(0.0, 1.0);
        ui.add(egui::ProgressBar::new(progress).text(format!("Evolution Level: {:.0}%", progress * 100.0)));
    });

    ui.add_space(30.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("☁").size(24.0).color(Color32::from_rgb(0, 150, 255)));
        ui.label(RichText::new("GENETIC CLOUD PEERS").strong());
    });

    ui.group(|ui| {
        if app.discovered_sidecars.is_empty() {
            ui.label("Scanning for local peers...");
        } else {
            for peer in &app.discovered_sidecars {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📡").color(Color32::GREEN));
                    ui.vertical(|ui| {
                        ui.label(&peer.name);
                        ui.label(RichText::new(format!("{} - {}", peer.author, peer.version)).size(10.0).color(Color32::GRAY));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("SYNC DNA").clicked() {
                            // Logic would trigger mDNS/TCP sync
                        }
                    });
                });
                ui.separator();
            }
        }
    });

    ui.add_space(30.0);
    if ui.button("EXPORT GENETIC PASSPORT").clicked() {
        // Future feature: Export DNA signature
    }
}
