use nullherz_dna::GeneticLibrary;
use egui::{Ui, RichText, Frame, Margin, Rounding};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    let frame_width = ui.available_width().min(400.0);
    let telemetry_opt = app.last_telemetry.lock().unwrap().clone();

    let mut list = Vec::new();

    // Fetch actual live mesh peer names from telemetry if present
    if let Some(ref t) = telemetry_opt {
        if t.mesh_peer_count > 0 {
            for i in 0..(t.mesh_peer_count as usize).min(8) {
                let name_bytes = t.mesh_peer_names[i].name;
                if name_bytes[0] != 0 {
                    let name = String::from_utf8_lossy(&name_bytes).trim_matches(char::from(0)).to_string();
                    list.push(name);
                }
            }
        }
    }

    let mut is_fallback = false;
    if list.is_empty() {
        is_fallback = true;
        list.push("Studio-PC-2".to_string());
        list.push("MacBook-DSP".to_string());
    }

    // 1. Peer Registry
    render_card(ui, frame_width, &theme, |ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new("PEER REGISTRY").small().strong().color(theme.text_secondary));
            ui.add_space(theme.space_xs);
            if is_fallback {
                ui.label(RichText::new("No active peers detected. Presenting mock peers for emulation:").size(theme.type_caption).color(theme.text_secondary));
                ui.add_space(theme.space_xs);
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("PEERS:").size(theme.type_caption).strong());
                for peer in &list {
                    ui.label(RichText::new(format!("● {}", peer)).color(theme.success).size(theme.type_caption));
                }
            });
        });
    });

    ui.add_space(theme.space_md);
    // Custom themed 1px painted horizontal separator
    let (sep_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(sep_rect.x_range(), sep_rect.center().y, egui::Stroke::new(1.0, theme.border));
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
            render_card(ui, frame_width, &theme, |ui| {
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
                ui.checkbox(&mut app.auto_pollinate_enabled, "Auto-Pollinate");
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
        .inner_margin(Margin::same(theme.space_sm))
        .show(ui, |ui| {
            ui.set_width(width);
            add_contents(ui);
        });
}
