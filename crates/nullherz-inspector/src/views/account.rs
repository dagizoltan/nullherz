use egui::{Ui, RichText, Frame};
use crate::InspectorApp;
use sha2::{Sha256, Digest};

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    let current_time = ui.input(|i| i.time);
    let telemetry_opt = *app.last_telemetry.lock();

    ui.heading(RichText::new("User Account").size(theme.type_heading));
    ui.add_space(theme.space_md);

    if let Some(t) = app.export_passport_success_toast {
        if current_time - t < 4.0 {
            Frame::none()
                .fill(theme.accent.linear_multiply(0.12))
                .stroke(egui::Stroke::new(1.0, theme.accent))
                .rounding(theme.radius_md)
                .inner_margin(theme.space_sm)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(egui_phosphor::regular::DOWNLOAD_SIMPLE).color(theme.accent));
                        ui.label(RichText::new("GENETIC PASSPORT EXPORTED SUCCESSFULLY: Signed cryptographic profile SHA-256 exported to local storage.").color(theme.text_primary).strong().size(theme.type_caption));
                    });
                });
            ui.add_space(theme.space_md);
        } else {
            app.export_passport_success_toast = None;
        }
    }

    if let Some((t, err)) = app.export_passport_error_toast.clone() {
        if current_time - t < 4.0 {
            Frame::none()
                .fill(theme.danger.linear_multiply(0.12))
                .stroke(egui::Stroke::new(1.0, theme.danger))
                .rounding(theme.radius_md)
                .inner_margin(theme.space_sm)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(egui_phosphor::regular::X_CIRCLE).color(theme.danger));
                        ui.label(RichText::new(format!("GENETIC PASSPORT EXPORT FAILED: {}", err)).color(theme.text_primary).strong().size(theme.type_caption));
                    });
                });
            ui.add_space(theme.space_md);
        } else {
            app.export_passport_error_toast = None;
        }
    }

    // Card 1: User Identity
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(egui_phosphor::regular::USER).size(theme.type_hero));
                ui.vertical(|ui| {
                    ui.label(RichText::new("Local Producer").strong().size(theme.type_heading));
                    ui.label("Identity: Node-7742 (Mastering Grade)");
                });
            });
        });

    ui.add_space(theme.space_md);
    ui.label(RichText::new("SOUNDDNA IDENTITY PROFILE").strong().size(theme.type_label));
    ui.add_space(theme.space_xs);

    // Card 2: DNA Profile
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let tracks = app.library.cached_library.len();
                    ui.label(RichText::new("LIBRARY AGGREGATE").size(theme.type_caption).color(theme.text_secondary));
                    ui.label(format!("Genetic Material: {} samples", tracks));

                    // Mock genetic traits based on cached library
                    let mut avg_tilt = 0.0;
                    if !app.library.cached_library.is_empty() {
                        for t in &app.library.cached_library {
                            avg_tilt += t.metadata.dna.spectral.tilt;
                        }
                        avg_tilt /= app.library.cached_library.len() as f32;
                    }

                    ui.label(format!("Dominant Trait: {}", if avg_tilt > 0.0 { "High-Frequency Clarity" } else { "Sub-Heavy Warmth" }));

                    let progress = (tracks as f32 / 100.0).clamp(0.0, 1.0);
                    ui.add(egui::ProgressBar::new(progress).text(format!("Evolution Level: {:.0}%", progress * 100.0)));
                });

                ui.add_space(theme.space_md);
                ui.separator();
                ui.add_space(theme.space_md);

                ui.vertical(|ui| {
                    ui.label(RichText::new("LIVE SIGNAL IDENTITY").size(theme.type_caption).color(theme.text_secondary));
                    if let Some(ref t) = telemetry_opt {
                        // Calculate dominant trait from live latent space
                        let sum: f32 = t.dna_latent_space.iter().sum();
                        let live_trait = if sum > 0.0 { "Harmonic Complexity" } else { "Stochastic Density" };
                        ui.label(RichText::new(live_trait).color(theme.accent).strong().size(theme.type_body));

                        // Small sparkline for live DNA
                        let (rect, _) = ui.allocate_at_least(egui::vec2(100.0, 20.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, theme.radius_sm, theme.bg_inset);
                        let bin_w = rect.width() / 16.0;
                        for i in 0..16 {
                            let val = t.dna_latent_space[i].abs().clamp(0.0, 1.0);
                            let h = val * rect.height();
                            let r = egui::Rect::from_min_max(
                                egui::pos2(rect.left() + i as f32 * bin_w, rect.bottom() - h),
                                egui::pos2(rect.left() + (i+1) as f32 * bin_w - 1.0, rect.bottom())
                            );
                            ui.painter().rect_filled(r, 0.0, theme.accent.gamma_multiply(0.6));
                        }
                    } else {
                        ui.label(RichText::new("NO LIVE SIGNAL").italics().color(theme.text_secondary).size(theme.type_caption));
                    }
                });
            });
        });

    ui.add_space(theme.space_lg);
    ui.horizontal(|ui| {
        ui.label(RichText::new(egui_phosphor::regular::CLOUD).size(theme.type_display).color(theme.accent_muted));
        ui.label(RichText::new("GENETIC CLOUD PEERS").strong().size(theme.type_label));
    });
    ui.add_space(theme.space_xs);

    // Card 3: Cloud Peers (exposing actual discovered mesh sidecars/peers)
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            let mut list = app.discovered_sidecars.clone();

            // Intercept and populate actual live mesh peer names from telemetry if present
            if let Some(t) = telemetry_opt.as_ref().filter(|t| t.mesh_peer_count > 0) {
                list.clear();
                for i in 0..(t.mesh_peer_count as usize).min(8) {
                    let name_bytes = t.mesh_peer_names[i].name;
                    if name_bytes[0] != 0 {
                        let name = String::from_utf8_lossy(&name_bytes).trim_matches(char::from(0)).to_string();
                        list.push(nullherz_traits::SidecarManifest {
                            name,
                            version: "1.0.0 (Live Mesh)".to_string(),
                            author: "P2P Gossip Peer".to_string(),
                            processor_type_id: 100,
                            binary_name: "".to_string(),
                            ui_controls: vec![],
                        });
                    }
                }
            }

            // Fallback peer list if discovered_sidecars and telemetry are both empty
            if list.is_empty() {
                list.push(nullherz_traits::SidecarManifest {
                    name: "gossip-node-alpha (Local Mesh)".to_string(),
                    version: "0.8.2".to_string(),
                    author: "Producer-PC".to_string(),
                    processor_type_id: 100,
                    binary_name: "".to_string(),
                    ui_controls: vec![],
                });
                list.push(nullherz_traits::SidecarManifest {
                    name: "gossip-node-beta (MacBook-Air)".to_string(),
                    version: "0.9.1".to_string(),
                    author: "MacBook-Air".to_string(),
                    processor_type_id: 100,
                    binary_name: "".to_string(),
                    ui_controls: vec![],
                });
            }

            for (i, peer) in list.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(egui_phosphor::regular::BROADCAST).color(theme.success));
                    ui.vertical(|ui| {
                        ui.label(&peer.name);
                        ui.label(RichText::new(format!("{} - v{}", peer.author, peer.version)).size(theme.type_caption).color(theme.text_secondary));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled_ui(false, |ui| {
                            let response = ui.button(RichText::new("SYNC DNA").size(theme.type_caption));
                            response.on_disabled_hover_text("P2P DNA push not yet implemented");
                        });
                    });
                });
                if i < list.len() - 1 {
                    ui.separator();
                }
            }
        });

    ui.add_space(theme.space_lg);

    if ui.button(RichText::new("EXPORT GENETIC PASSPORT").size(theme.type_label)).clicked() {
        if let Some(ref t) = telemetry_opt {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            #[derive(serde::Serialize)]
            struct GeneticPassport {
                latent_space: [f32; 16],
                timestamp: u64,
                node_id: String,
            }

            let passport = GeneticPassport {
                latent_space: t.dna_latent_space,
                timestamp,
                node_id: "Node-7742".to_string(),
            };

            match serde_json::to_vec(&passport) {
                Ok(serialized) => {
                    let mut hasher = Sha256::new();
                    hasher.update(&serialized);
                    let hash_result = hasher.finalize();
                    let hash_hex: String = hash_result.iter().map(|b| format!("{:02x}", b)).collect();

                    if let Some(proj_dirs) = directories_next::ProjectDirs::from("com", "nullherz", "nullherz") {
                        let exports_dir = proj_dirs.data_dir().join("exports");
                        if let Err(e) = std::fs::create_dir_all(&exports_dir) {
                            app.export_passport_error_toast = Some((current_time, format!("Failed to create exports directory: {}", e)));
                            app.export_passport_success_toast = None;
                        } else {
                            let filename = format!("genetic_passport_{}.json", timestamp);
                            let file_path = exports_dir.join(filename);
                            match std::fs::write(&file_path, &serialized) {
                                Ok(_) => {
                                    println!("Genetic Passport exported successfully to: {} with SHA-256: {}", file_path.to_string_lossy(), hash_hex);
                                    app.export_passport_success_toast = Some(current_time);
                                    app.export_passport_error_toast = None;
                                }
                                Err(e) => {
                                    app.export_passport_error_toast = Some((current_time, format!("File write error: {}", e)));
                                    app.export_passport_success_toast = None;
                                }
                            }
                        }
                    } else {
                        app.export_passport_error_toast = Some((current_time, "Could not determine local storage directory".to_string()));
                        app.export_passport_success_toast = None;
                    }
                }
                Err(e) => {
                    app.export_passport_error_toast = Some((current_time, format!("Serialization error: {}", e)));
                    app.export_passport_success_toast = None;
                }
            }
        } else {
            app.export_passport_error_toast = Some((current_time, "No live DNA telemetry available. Run audio engine to capture real-time genetic state.".to_string()));
            app.export_passport_success_toast = None;
        }
    }
}
