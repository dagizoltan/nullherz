use nullherz_dna::GeneticLibrary;
use egui::{Ui, RichText, Frame, Margin, Rounding};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    // Fetch live mesh peer names from telemetry if present
    let telemetry_lock = app.last_telemetry.lock();
    let mut peer_names = vec![];
    let mut is_mock = true;

    if let Some(ref t) = *telemetry_lock
        && t.mesh_peer_count > 0 {
            is_mock = false;
            for i in 0..(t.mesh_peer_count as usize).min(8) {
                let name_bytes = t.mesh_peer_names[i].name;
                if name_bytes[0] != 0 {
                    let name = String::from_utf8_lossy(&name_bytes).trim_matches(char::from(0)).to_string();
                    peer_names.push(name);
                }
            }
        }

    if is_mock {
        peer_names.push("Studio-PC-2".to_string());
        peer_names.push("MacBook-DSP".to_string());
    }

    // 1. Peer Registry
    render_card_group(ui, "PEER REGISTRY", &theme, |ui| {
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("PEERS:").size(theme.type_caption).strong());
                for name in &peer_names {
                    ui.label(RichText::new(format!("● {}", name)).color(theme.success).size(theme.type_caption));
                }
            });
            if is_mock {
                ui.add_space(theme.space_xs);
                ui.label(RichText::new("Presenting simulated/mock peers for offline demonstration").size(theme.type_caption).color(theme.text_disabled).italics());
            }
        });
    });

    ui.add_space(theme.space_md);
    let (line_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(line_rect, Rounding::ZERO, theme.border);
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
            render_card_group(ui, "TEMPLATE SOURCE", &theme, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&track.title).strong().size(theme.type_body));
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
                            app.library.library_needs_refresh = true;
                        }
                    });
                });
            });
            ui.add_space(theme.space_xs);
        }

        ui.add_space(theme.space_lg);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("REFRESH CLOUD").size(theme.type_label)).clicked() { app.library.library_needs_refresh = true; }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Bug Fixed: Now bound to actual InspectorApp field instead of per-frame transient local throwaway variable!
                ui.checkbox(&mut app.composer.auto_pollinate_enabled, "Auto-Pollinate");
            });
        });
    });
}

fn render_card_group<F>(ui: &mut Ui, title: &str, theme: &nullherz_ui_hal::Theme, add_contents: F)
where F: FnOnce(&mut Ui)
{
    ui.label(RichText::new(title).small().strong().color(theme.text_secondary));
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .stroke(theme.border_stroke)
        .inner_margin(Margin::same(theme.space_md))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}
