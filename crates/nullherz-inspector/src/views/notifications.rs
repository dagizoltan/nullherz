use nullherz_dna::GeneticLibrary;
use egui::{Ui, ScrollArea, RichText, Frame, Margin, Rounding};
use crate::InspectorApp;

pub fn render(app: &InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    let frame_width = ui.available_width().min(400.0);
    let telemetry = app.last_telemetry.lock().unwrap();

    ScrollArea::vertical().id_source("ai_scroll").show(ui, |ui| {
        ui.vertical(|ui| {
            // High Confidence Suggestions
            ui.label(RichText::new("HIGH CONFIDENCE MATCHES").small().strong().color(theme.accent));
            ui.add_space(theme.space_xs);

            if let Some(t) = &*telemetry {
                let mut has_suggestions = false;
                for (id, score) in t.suggestions {
                    if id == 0 || score < 0.7 { continue; }
                    has_suggestions = true;

                    let track = app.library_db.get_track(id).ok().flatten();
                    let title = track.as_ref().map(|tr| tr.title.as_str()).unwrap_or("Unknown");

                    render_card(ui, frame_width, &theme, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(format!("{} ({:.0}%)", title, score * 100.0));
                                ui.label(RichText::new("Perfect candidate for spectral transfusion.").size(theme.type_caption).color(theme.text_secondary));
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
                    ui.add_space(theme.space_xs);
                }
                if !has_suggestions {
                    ui.label(RichText::new("No high-confidence matches found.").small().italics().color(theme.text_secondary));
                }
            }

            ui.add_space(theme.space_md);
            ui.label(RichText::new("GENETIC DRIFT (DECK DISTANCE)").small().strong().color(theme.text_secondary));
            let panel_width = ui.available_width().min(frame_width);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(panel_width, 60.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, theme.radius_md, theme.bg_inset);
            ui.painter().rect_stroke(rect, theme.radius_md, theme.border_stroke);

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
                ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.5, theme.accent)));
                ui.painter().text(rect.left_top() + egui::vec2(5.0, 5.0), egui::Align2::LEFT_TOP, "STABLE", egui::FontId::monospace(theme.type_caption), theme.text_secondary);
            }

            ui.add_space(theme.space_md);
            render_card(ui, frame_width, &theme, |ui| {
                ui.label(RichText::new("SYSTEM EVENTS").strong());
                ui.add_space(theme.space_xs);
                ui.label(RichText::new("Presenting mock events for emulation:").size(theme.type_caption).color(theme.text_secondary));
                ui.separator();
                ui.label("• Sidecar #1: Performance stable.");
                ui.label("• Cloud Sync: 4 new templates discovered.");
            });
        });
    });
}

fn render_card<F>(ui: &mut Ui, width: f32, theme: &nullherz_ui_hal::Theme, add_contents: F)
where F: FnOnce(&mut Ui)
{
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .stroke(theme.border_stroke)
        .inner_margin(Margin::same(theme.space_md))
        .show(ui, |ui| {
            ui.set_width(width);
            add_contents(ui);
        });
}
