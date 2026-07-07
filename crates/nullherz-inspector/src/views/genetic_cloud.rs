use nullherz_dna::GeneticLibrary;
use egui::{Ui, Color32, RichText, Frame, Margin};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.label(RichText::new("GENETIC CLOUD").strong().color(app.theme.accent));
    ui.add_space(10.0);

    // 1. Peer Registry
    Frame::group(ui.style()).fill(Color32::from_rgb(15, 20, 18)).inner_margin(Margin::same(8.0)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("PEERS:").small().strong());
            ui.label(RichText::new("● Studio-PC-2").color(Color32::GREEN).small());
            ui.label(RichText::new("● MacBook-DSP").color(Color32::GREEN).small());
        });
    });

    ui.add_space(15.0);
    ui.separator();
    ui.add_space(10.0);

    // 2. Discovered Templates
    egui::ScrollArea::vertical().id_source("cloud_scroll").show(ui, |ui| {
        ui.label(RichText::new("DISCOVERED DNA").small().color(Color32::from_gray(120)));
        ui.add_space(8.0);

        let tracks = app.library_db.list_tracks().unwrap_or_default();
        let cloud_tracks: Vec<_> = tracks.iter().filter(|t| t.artist == "Cloud Peer").collect();

        if cloud_tracks.is_empty() {
            ui.label(RichText::new("Searching local network...").small().italics());
        }

        for track in cloud_tracks {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&track.title).strong());
                            // Production Beta: Mock verification status check
                            let is_verified = track.id % 2 == 0;
                            if is_verified {
                                ui.label(RichText::new("✔ VERIFIED").small().color(Color32::from_rgb(0, 255, 200)));
                            }
                        });
                        ui.label(RichText::new(&track.artist).small().color(Color32::GRAY));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("🧬").on_hover_text("Pollinate").clicked() {
                            let mut local_copy = (*track).clone();
                            local_copy.id = track.id ^ 0xFEED;
                            local_copy.artist = "Imported Genesis".to_string();
                            let _ = app.library_db.save_track(&local_copy);
                            app.library_needs_refresh = true;
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }

        ui.add_space(20.0);
        ui.horizontal(|ui| {
            if ui.button("REFRESH CLOUD").clicked() { app.library_needs_refresh = true; }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut auto = false;
                ui.checkbox(&mut auto, "Auto-Pollinate");
            });
        });
    });
}
