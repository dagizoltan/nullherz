use egui::{Ui, ScrollArea, Color32, RichText};
use crate::InspectorApp;

pub fn render(app: &InspectorApp, ui: &mut Ui) {
    ui.heading(RichText::new("AI ANALYSIS").strong().color(Color32::from_rgb(0, 255, 200)));
    ui.add_space(10.0);

    let telemetry = app.last_telemetry.lock().unwrap();

    ScrollArea::vertical().show(ui, |ui| {
        ui.vertical(|ui| {
            // AI Suggestions Section
            ui.group(|ui| {
                ui.label(RichText::new("TRANSFUSION SUGGESTIONS").strong());
                ui.separator();

                if let Some(t) = &*telemetry {
                    let mut has_suggestions = false;
                    for (id, score) in t.suggestions {
                        if id == 0 { continue; }
                        has_suggestions = true;

                        let track = app.library_db.get_track(id).ok().flatten();
                        let title = track.as_ref().map(|tr| tr.title.as_str()).unwrap_or("Unknown");

                        ui.vertical(|ui| {
                            ui.label(format!("Match: {} ({:.0}%)", title, score * 100.0));
                            ui.label(RichText::new("High genetic compatibility detected. Try a 50% Transfusion?").size(9.0).color(Color32::GRAY));
                            ui.horizontal(|ui| {
                                if ui.button(RichText::new("LOAD TO DECK B").small()).clicked() {
                                    let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::AddSourceFromRegistry {
                                        granular_node_idx: 4, // Deck B
                                        sample_id: id,
                                    }));
                                }
                            });
                        });
                        ui.add_space(8.0);
                    }
                    if !has_suggestions {
                        ui.label(RichText::new("Analyzing library for matches...").small().italics());
                    }
                } else {
                    ui.label(RichText::new("Connect engine for AI insights.").small().italics());
                }
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
