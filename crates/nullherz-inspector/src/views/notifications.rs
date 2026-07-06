use nullherz_dna::GeneticLibrary;
use egui::{Ui, ScrollArea, Color32, RichText, Frame, Margin};
use crate::InspectorApp;

pub fn render(app: &InspectorApp, ui: &mut Ui) {
    ui.heading(RichText::new("AI & INSIGHTS").strong().color(Color32::from_rgb(0, 255, 200)));
    ui.add_space(10.0);

    let telemetry = app.last_telemetry.lock().unwrap();

    ScrollArea::vertical().id_source("ai_scroll").show(ui, |ui| {
        ui.vertical(|ui| {
            // High Confidence Suggestions
            ui.label(RichText::new("HIGH CONFIDENCE MATCHES").small().strong().color(Color32::from_rgb(0, 255, 150)));
            ui.add_space(5.0);

            if let Some(t) = &*telemetry {
                let mut has_suggestions = false;
                for (id, score) in t.suggestions {
                    if id == 0 || score < 0.7 { continue; }
                    has_suggestions = true;

                    let track = app.library_db.get_track(id).ok().flatten();
                    let title = track.as_ref().map(|tr| tr.title.as_str()).unwrap_or("Unknown");

                    Frame::group(ui.style()).fill(Color32::from_rgb(20, 25, 22)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(format!("{} ({:.0}%)", title, score * 100.0));
                                ui.label(RichText::new("Perfect candidate for spectral transfusion.").size(9.0).color(Color32::GRAY));
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
                    ui.label(RichText::new("No high-confidence matches found.").small().italics().color(Color32::from_gray(100)));
                }
            }

            ui.add_space(15.0);
            ui.label(RichText::new("GENETIC DRIFT").small().strong().color(Color32::from_gray(120)));
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(15, 15, 20));
            ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, Color32::from_gray(30)));

            // Render a stylized drift wave (Mocked)
            let mut points = vec![];
            for i in 0..20 {
                let x = rect.left() + (i as f32 / 19.0) * rect.width();
                let y = rect.center().y + ((i as f32 * 0.8).sin() * 10.0);
                points.push(egui::pos2(x, y));
            }
            ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.5, Color32::from_rgb(0, 200, 255))));

            ui.add_space(15.0);
            ui.group(|ui| {
                ui.label(RichText::new("SYSTEM EVENTS").strong());
                ui.separator();
                ui.label("• Sidecar #1: Performance stable.");
                ui.label("• Cloud Sync: 4 new templates discovered.");
            });
        });
    });
}
