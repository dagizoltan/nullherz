use egui::{Ui, RichText, Frame};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.heading(RichText::new("User Account").size(theme.type_heading));
    ui.add_space(theme.space_md);

    // Card 1: User Identity
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(egui_phosphor::regular::USER).size(theme.type_hero));
                ui.vertical(|ui| {
                    ui.label(RichText::new("Local Producer").strong().size(theme.type_heading));
                    ui.label("Identity: Node-7742 (Mastering Grade)");
                });
            });
        });

    ui.add_space(theme.space_md);
    ui.label(RichText::new("SOUNDDNA IDENTITY PROFILE").strong().size(theme.type_label));
    ui.add_space(theme.space_xs);

    // Card 2: DNA Profile
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let tracks = app.cached_library.len();
                    ui.label(RichText::new("LIBRARY AGGREGATE").size(theme.type_caption).color(theme.text_secondary));
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

                ui.add_space(theme.space_md);
                ui.separator();
                ui.add_space(theme.space_md);

                ui.vertical(|ui| {
                    ui.label(RichText::new("LIVE SIGNAL IDENTITY").size(theme.type_caption).color(theme.text_secondary));
                    let telemetry = app.last_telemetry.lock().unwrap().clone();
                    if let Some(t) = telemetry {
                        // Calculate dominant trait from live latent space
                        let sum: f32 = t.dna_latent_space.iter().sum();
                        let live_trait = if sum > 0.0 { "Harmonic Complexity" } else { "Stochastic Density" };
                        ui.label(RichText::new(live_trait).color(theme.accent).strong().size(theme.type_body));

                        // Small sparkline for live DNA
                        let (rect, _) = ui.allocate_at_least(egui::vec2(100.0, 20.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, theme.radius_sm, theme.bg_inset);
                        let bin_w = rect.width() / 16.0;
                        for i in 0..16 {
                            let val = t.dna_latent_space[i].abs().clamp(0.0, 1.0);
                            let h = val * rect.height();
                            let r = egui::Rect::from_min_max(
                                egui::pos2(rect.left() + i as f32 * bin_w, rect.bottom() - h),
                                egui::pos2(rect.left() + (i+1) as f32 * bin_w - 1.0, rect.bottom())
                            );
                            ui.painter().rect_filled(r, 0.0, theme.accent.gamma_multiply(0.6));
                        }
                    } else {
                        ui.label(RichText::new("NO LIVE SIGNAL").italics().color(theme.text_secondary).size(theme.type_caption));
                    }
                });
            });
        });

    ui.add_space(theme.space_lg);
    ui.horizontal(|ui| {
        ui.label(RichText::new(egui_phosphor::regular::CLOUD).size(theme.type_display).color(theme.accent_muted));
        ui.label(RichText::new("GENETIC CLOUD PEERS").strong().size(theme.type_label));
    });
    ui.add_space(theme.space_xs);

    // Card 3: Cloud Peers
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            if app.discovered_sidecars.is_empty() {
                ui.label("Scanning for local peers...");
            } else {
                for peer in &app.discovered_sidecars {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(egui_phosphor::regular::BROADCAST).color(theme.success));
                        ui.vertical(|ui| {
                            ui.label(&peer.name);
                            ui.label(RichText::new(format!("{} - {}", peer.author, peer.version)).size(theme.type_caption).color(theme.text_secondary));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Wire the SYNC DNA button to be disabled until P2P/mesh integration completes
                            ui.add_enabled_ui(false, |ui| {
                                ui.button(RichText::new("SYNC DNA").size(theme.type_caption))
                                    .on_disabled_hover_text("Coming soon in Production Beta");
                            });
                        });
                    });
                    ui.separator();
                }
            }
        });

    ui.add_space(theme.space_lg);

    // Wire the EXPORT GENETIC PASSPORT button to be disabled with hover text
    ui.add_enabled_ui(false, |ui| {
        ui.button(RichText::new("EXPORT GENETIC PASSPORT").size(theme.type_label))
            .on_disabled_hover_text("Coming soon in Production Beta");
    });
}
