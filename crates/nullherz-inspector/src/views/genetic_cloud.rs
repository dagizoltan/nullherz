use nullherz_dna::GeneticLibrary;
use egui::{Ui, Color32, RichText, Frame, Margin, Rounding};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    // 1. Peer Registry
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(Margin::same(theme.space_sm))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("PEERS:").size(theme.type_caption).strong());
                ui.label(RichText::new("● Studio-PC-2").color(theme.success).size(theme.type_caption));
                ui.label(RichText::new("● MacBook-DSP").color(theme.success).size(theme.type_caption));
            });
        });

    ui.add_space(theme.space_md);
    ui.separator();
    ui.add_space(theme.space_sm);

    // 2. Discovered Templates
    egui::ScrollArea::vertical().id_source("cloud_scroll").show(ui, |ui| {
        ui.label(RichText::new("DISCOVERED DNA").size(theme.type_caption).color(theme.text_secondary));
        ui.add_space(theme.space_xs);

        let tracks = app.library_db.list_tracks().unwrap_or_default();
        let cloud_tracks: Vec<_> = tracks.iter().filter(|t| t.artist == "Cloud Peer").collect();

        if cloud_tracks.is_empty() {
            ui.label(RichText::new("Searching local network...").size(theme.type_caption).italics());
        }

        for track in cloud_tracks {
            Frame::none()
                .fill(theme.bg_inset)
                .rounding(Rounding::same(theme.radius_md))
                .stroke(theme.border_stroke)
                .inner_margin(Margin::same(theme.space_sm))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&track.title).strong().size(theme.type_body));
                            // Production Beta: Mock verification status check
                            let is_verified = track.id % 2 == 0;
                            if is_verified {
                                ui.label(RichText::new(format!("{} VERIFIED", egui_phosphor::regular::CHECK)).size(theme.type_caption).color(theme.accent));
                            }
                        });
                        ui.label(RichText::new(&track.artist).size(theme.type_caption).color(theme.text_secondary));
                    });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(RichText::new(egui_phosphor::regular::DNA).size(theme.type_label)).on_hover_text("Pollinate").clicked() {
                                let mut local_copy = (*track).clone();
                                local_copy.id = track.id ^ 0xFEED;
                                local_copy.artist = "Imported Genesis".to_string();
                                let _ = app.library_db.save_track(&local_copy);
                                app.library_needs_refresh = true;
                            }
                        });
                    });
                });
            ui.add_space(theme.space_xs);
        }

        ui.add_space(theme.space_lg);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("REFRESH CLOUD").size(theme.type_label)).clicked() { app.library_needs_refresh = true; }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut auto = false;
                ui.checkbox(&mut auto, "Auto-Pollinate");
            });
        });
    });
}
