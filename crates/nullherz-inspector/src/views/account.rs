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
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                let tracks = app.cached_library.len();
                ui.label(RichText::new("LIBRARY AGGREGATE").small().color(Color32::from_gray(120)));
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

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(20.0);

            ui.vertical(|ui| {
                ui.label(RichText::new("LIVE SIGNAL IDENTITY").small().color(Color32::from_gray(120)));
                let telemetry = app.last_telemetry.lock().unwrap().clone();
                if let Some(t) = telemetry {
                    // Calculate dominant trait from live latent space
                    let sum: f32 = t.dna_latent_space.iter().sum();
                    let live_trait = if sum > 0.0 { "Harmonic Complexity" } else { "Stochastic Density" };
                    ui.label(RichText::new(live_trait).color(app.theme.accent).strong());

                    // Small sparkline for live DNA
                    let (rect, _) = ui.allocate_at_least(egui::vec2(100.0, 20.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 1.0, Color32::from_black_alpha(100));
                    let bin_w = rect.width() / 16.0;
                    for i in 0..16 {
                        let val = t.dna_latent_space[i].abs().clamp(0.0, 1.0);
                        let h = val * rect.height();
                        let r = egui::Rect::from_min_max(
                            egui::pos2(rect.left() + i as f32 * bin_w, rect.bottom() - h),
                            egui::pos2(rect.left() + (i+1) as f32 * bin_w - 1.0, rect.bottom())
                        );
                        ui.painter().rect_filled(r, 0.0, app.theme.accent.gamma_multiply(0.6));
                    }
                } else {
                    ui.label(RichText::new("NO LIVE SIGNAL").italics().color(Color32::GRAY));
                }
            });
        });
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
