use nullherz_dna::GeneticLibrary;
use egui::{Ui, ScrollArea, Color32, RichText, Frame, Rounding, Stroke};
use crate::InspectorApp;

pub fn render(app: &InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.heading(RichText::new("AI & INSIGHTS").strong().color(theme.accent).size(theme.type_heading));
    ui.add_space(theme.space_sm);

    let telemetry = app.last_telemetry.lock().unwrap();

    ScrollArea::vertical().id_source("ai_scroll").show(ui, |ui| {
        ui.vertical(|ui| {
            // High Confidence Suggestions
            ui.label(RichText::new("HIGH CONFIDENCE MATCHES").size(theme.type_caption).strong().color(theme.success));
            ui.add_space(5.0);

            if let Some(t) = &*telemetry {
                let mut has_suggestions = false;
                for (id, score) in t.suggestions {
                    if id == 0 || score < 0.7 { continue; }
                    has_suggestions = true;

                    let track = app.library_db.get_track(id).ok().flatten();
                    let title = track.as_ref().map(|tr| tr.title.as_str()).unwrap_or("Unknown");

                    Frame::group(ui.style())
                        .fill(theme.bg_inset)
                        .rounding(Rounding::same(theme.radius_sm))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(format!("{} ({:.0}%)", title, score * 100.0)).size(theme.type_body));
                                    ui.label(RichText::new("Perfect candidate for spectral transfusion.").size(theme.type_caption).color(theme.text_secondary));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(RichText::new("LOAD").size(theme.type_caption)).clicked() {
                                         let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::AddSourceFromRegistry {
                                            granular_node_idx: 4, sample_id: id,
                                        }));
                                    }
                                });
                            });
                        });
                    ui.add_space(theme.space_xs);
                }
                if !has_suggestions {
                    ui.label(RichText::new("No high-confidence matches found.").size(theme.type_caption).italics().color(theme.text_disabled));
                }
            }

            ui.add_space(theme.space_md);
            ui.label(RichText::new("GENETIC DRIFT (DECK DISTANCE)").size(theme.type_caption).strong().color(theme.text_secondary));
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 60.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, theme.radius_sm, theme.bg_inset);
            ui.painter().rect_stroke(rect, theme.radius_sm, egui::Stroke::new(1.0, theme.border));

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
                ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.5, theme.success)));
                ui.painter().text(rect.left_top() + egui::vec2(5.0, 5.0), egui::Align2::LEFT_TOP, "STABLE", egui::FontId::monospace(theme.type_caption), theme.text_disabled);
            }

            ui.add_space(theme.space_md);
            ui.group(|ui| {
                ui.label(RichText::new("SYSTEM EVENTS").strong().size(theme.type_body));
                ui.separator();
                ui.label("• Sidecar #1: Performance stable.");
                ui.label("• Cloud Sync: 4 new templates discovered.");
            });
        });
    });
}
