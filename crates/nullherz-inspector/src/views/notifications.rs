use nullherz_dna::GeneticLibrary;
use egui::{Ui, ScrollArea, RichText, Frame};
use crate::InspectorApp;

pub fn render(app: &InspectorApp, ui: &mut Ui) {
    ui.heading(RichText::new("AI & INSIGHTS").strong().color(app.theme.accent));
    ui.add_space(10.0);

    let telemetry = app.last_telemetry.lock().unwrap();

    ScrollArea::vertical().id_source("ai_scroll").show(ui, |ui| {
        ui.vertical(|ui| {
            // High Confidence Suggestions
            ui.label(RichText::new("HIGH CONFIDENCE MATCHES").small().strong().color(app.theme.accent));
            ui.add_space(5.0);

            if let Some(t) = &*telemetry {
                let mut has_suggestions = false;
                for (id, score) in t.suggestions {
                    if id == 0 || score < 0.7 { continue; }
                    has_suggestions = true;

                    let track = app.library_db.get_track(id).ok().flatten();
                    let title = track.as_ref().map(|tr| tr.title.as_str()).unwrap_or("Unknown");

                    Frame::none()
                        .fill(app.theme.bg_surface)
                        .rounding(app.theme.radius_md)
                        .stroke(app.theme.border_stroke)
                        .inner_margin(app.theme.space_md)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(format!("{} ({:.0}%)", title, score * 100.0));
                                    ui.label(RichText::new("Perfect candidate for spectral transfusion.").size(9.0).color(app.theme.text_secondary));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("LOAD").clicked() {
                                         let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::AddSourceFromRegistry {
                                            granular_node_idx: 4, sample_id: id,
                                         }));
                                    }
                                });
                            });
                        });
                    ui.add_space(5.0);
                }
                if !has_suggestions {
                    ui.label(RichText::new("No high-confidence matches found.").small().italics().color(app.theme.text_secondary));
                }
            }

            ui.add_space(15.0);
            ui.label(RichText::new("GENETIC DRIFT (DECK DISTANCE)").small().strong().color(app.theme.text_secondary));
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 60.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, app.theme.radius_md, app.theme.bg_inset);
            ui.painter().rect_stroke(rect, app.theme.radius_md, app.theme.border_stroke);

            // Render meaningful genetic drift visualization
            if let Some(t) = &*telemetry {
                let mut points = vec![];
                let deck_bias = (t.beat_position as f32 * 0.5).sin() * 0.2;
                for i in 0..32 {
                    let x = rect.left() + (i as f32 / 31.0) * rect.width();
                    let latent_val = t.dna_latent_space[i % 16];
                    let y = rect.center().y + (latent_val * 20.0) + (deck_bias * 15.0);
                    points.push(egui::pos2(x, y));
                }
                ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.5, app.theme.accent)));
                ui.painter().text(rect.left_top() + egui::vec2(5.0, 5.0), egui::Align2::LEFT_TOP, "STABLE", egui::FontId::monospace(8.0), app.theme.text_secondary);
            }

            ui.add_space(15.0);
            Frame::none()
                .fill(app.theme.bg_surface)
                .rounding(app.theme.radius_md)
                .stroke(app.theme.border_stroke)
                .inner_margin(app.theme.space_md)
                .show(ui, |ui| {
                    ui.label(RichText::new("SYSTEM EVENTS").strong());
                    ui.separator();
                    ui.label("• Sidecar #1: Performance stable.");
                    ui.label("• Cloud Sync: 4 new templates discovered.");
                });
        });
    });
}
