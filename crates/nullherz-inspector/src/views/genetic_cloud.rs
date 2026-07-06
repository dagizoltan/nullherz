use nullherz_dna::GeneticLibrary;
use egui::{Ui, Color32, RichText};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.label(RichText::new("GENETIC CLOUD").strong().color(Color32::from_rgb(0, 255, 200)));
    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label("P2P Genetic Discovery Active");
        ui.horizontal(|ui| {
            ui.label("Status:");
            ui.label(RichText::new("CONNECTED").color(Color32::GREEN).small());
        });
    });

    ui.add_space(20.0);
    ui.separator();
    ui.add_space(10.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label("Discovered genetic templates from the cloud:");
        ui.add_space(10.0);

        // Discovered peers and templates from the database/sync service
        let tracks = app.library_db.list_tracks().unwrap_or_default();
        let cloud_tracks: Vec<_> = tracks.iter().filter(|t| t.artist == "Cloud Peer").collect();

        if cloud_tracks.is_empty() {
            ui.label(RichText::new("No remote templates discovered yet. Ensure Conductor P2P is active.").small().italics());
        }

        for track in cloud_tracks {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&track.title).strong());
                    ui.label(RichText::new(format!("Origin: {}", track.artist)).small().color(Color32::GRAY));
                    ui.add_space(5.0);

                    if ui.button("🧬 POLLINATE").on_hover_text("Import this DNA into your local library").clicked() {
                        let mut local_copy = (*track).clone();
                        local_copy.id = track.id ^ 0xFEED; // New local ID
                        local_copy.artist = "Imported Genesis".to_string();
                        let _ = app.library_db.save_track(&local_copy);
                        app.library_needs_refresh = true;
                        println!("Pollinated local library with DNA template: {}", track.title);
                    }
                });
            });
            ui.add_space(8.0);
        }

        ui.add_space(20.0);
        if ui.button("REFRESH CLOUD").clicked() {
            app.library_needs_refresh = true;
        }
    });
}
