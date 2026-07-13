use egui::{Ui, Vec2, Stroke, Sense, RichText, Frame, Margin, Rounding};
use nullherz_traits::{Command, DnaCommand};
use nullherz_dna::GeneticLibrary;

pub struct BreederView {
    pub parent_a_id: Option<u64>,
    pub parent_b_id: Option<u64>,
    pub transfusion_bias_x: f32, // Spectral Bias / Interpolation
    pub transfusion_bias_y: f32, // Rhythmic Bias
    pub target_node_idx: u32,
    pub selecting_parent: Option<usize>, // 0 for A, 1 for B
    pub preview_dna: [f32; 16],
    pub _smoothed_goniometer: [f32; 128],
}

impl BreederView {
    pub fn new() -> Self {
        Self {
            parent_a_id: None,
            parent_b_id: None,
            transfusion_bias_x: 0.5,
            transfusion_bias_y: 0.5,
            target_node_idx: 150, // PersonalityInheritanceProcessor default ID
            selecting_parent: None,
            preview_dna: [0.0; 16],
            _smoothed_goniometer: [0.0; 128],
        }
    }

    pub fn show(ui: &mut Ui, state: &mut BreederView, telemetry: &Option<audio_core::Telemetry>, app: &mut crate::InspectorApp) {
        let theme = app.theme;
        ui.heading(RichText::new("DNA Breeder").size(theme.type_heading));
        ui.add_space(theme.space_sm);

        if let Some(parent_idx) = state.selecting_parent {
            egui::Window::new(format!("Select Parent {}", if parent_idx == 0 { "A" } else { "B" }))
                .collapsible(false).resizable(true).show(ui.ctx(), |ui| {

                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut app.search_query);
                    if ui.button("CLOSE").clicked() { state.selecting_parent = None; }
                });
                ui.separator();

                egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                    let mut tracks = app.cached_library.clone();

                    // Matchmaking Sort (Genetic Matchmaker UI)
                    let other_dna = if parent_idx == 0 {
                        state.parent_b_id.and_then(|id| app.library_db.get_track(id).ok().flatten()).map(|t| (*t.metadata).dna.clone())
                    } else {
                        state.parent_a_id.and_then(|id| app.library_db.get_track(id).ok().flatten()).map(|t| (*t.metadata).dna.clone())
                    };

                    if let Some(ref target) = other_dna {
                         tracks.sort_by(|a, b| {
                             let sa = nullherz_dna::calculate_similarity(target, &a.metadata.dna);
                             let sb = nullherz_dna::calculate_similarity(target, &b.metadata.dna);
                             sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
                         });
                    }

                    for track in tracks {
                        let similarity = other_dna.as_ref().map(|other| nullherz_dna::calculate_similarity(&track.metadata.dna, other));

                        ui.horizontal(|ui| {
                            if let Some(s) = similarity {
                                let color = if s > 0.8 {
                                    theme.accent
                                } else if s > 0.5 {
                                    theme.warning // Unified Yellow warning indicator
                                } else {
                                    theme.text_secondary
                                };
                                ui.add(egui::ProgressBar::new(s).desired_width(60.0).fill(color).text(RichText::new(format!("{:.0}%", s * 100.0)).size(theme.type_caption)));
                            }

                            let label = format!("{} - {}", track.title, track.artist);
                            if ui.selectable_label(false, RichText::new(label).size(theme.type_body)).clicked() {
                                if parent_idx == 0 {
                                    state.parent_a_id = Some(track.id);
                                } else {
                                    state.parent_b_id = Some(track.id);
                                }
                                app.library_needs_refresh = true;
                                state.selecting_parent = None;
                            }
                        });
                    }
                });
            });
        }

        ui.horizontal(|ui| {
            // Parent A Selection
            ui.vertical(|ui| {
                ui.label(RichText::new("Parent A").size(theme.type_caption).color(theme.text_secondary));
                let label = state.parent_a_id.and_then(|id| app.library_db.get_track(id).ok().flatten())
                    .map(|t| t.title).unwrap_or_else(|| "Select Sample".to_string());

                if ui.button(RichText::new(label).size(theme.type_body)).clicked() {
                    state.selecting_parent = Some(0);
                }
            });

            ui.add_space(theme.space_lg);
            ui.label(RichText::new("X").size(theme.type_heading).strong().color(theme.accent));
            ui.add_space(theme.space_lg);

            // Parent B Selection
            ui.vertical(|ui| {
                ui.label(RichText::new("Parent B").size(theme.type_caption).color(theme.text_secondary));
                let label = state.parent_b_id.and_then(|id| app.library_db.get_track(id).ok().flatten())
                    .map(|t| t.title).unwrap_or_else(|| "Select Sample".to_string());

                if ui.button(RichText::new(label).size(theme.type_body)).clicked() {
                    state.selecting_parent = Some(1);
                }
            });
        });

        ui.add_space(theme.space_lg);

        ui.horizontal(|ui| {
            // N-Dimensional DNA Breeder Map (PCA / t-SNE Canvas)
            ui.vertical(|ui| {
                ui.label(RichText::new("N-Dimensional DNA Breeder Map (t-SNE / PCA)").size(theme.type_body));
                let (rect, response) = ui.allocate_at_least(Vec2::splat(250.0), Sense::click_and_drag());

                ui.painter().rect_filled(rect, theme.radius_md, theme.bg_dark.linear_multiply(0.8));
                ui.painter().rect_stroke(rect, theme.radius_md, theme.border_stroke);

                // Grid lines (Industrial Look)
                for i in 1..4 {
                    let x = rect.left() + i as f32 * (rect.width() / 4.0);
                    ui.painter().vline(x, rect.y_range(), Stroke::new(0.5, theme.border.linear_multiply(0.5)));
                    let y = rect.top() + i as f32 * (rect.height() / 4.0);
                    ui.painter().hline(rect.x_range(), y, Stroke::new(0.5, theme.border.linear_multiply(0.5)));
                }

                // Dynamic template nodes (clustered templates from library)
                let mut tracks = app.cached_library.clone();
                if tracks.is_empty() {
                    if let Ok(list) = app.library_db.list_tracks() {
                        tracks = list;
                    }
                }

                let mut node_positions = Vec::new();
                for track in &tracks {
                    // Deterministic PCA/t-SNE layout coordinates inside bounds to keep nodes visible
                    let x_coord = 0.15 + ((track.id * 17) % 70) as f32 / 100.0;
                    let y_coord = 0.15 + ((track.id * 31) % 70) as f32 / 100.0;
                    let node_pos = rect.left_top() + Vec2::new(x_coord * rect.width(), (1.0 - y_coord) * rect.height());
                    node_positions.push((track.id, node_pos, track.title.clone()));

                    // Draw node dot
                    ui.painter().circle_filled(node_pos, 5.0, theme.text_secondary.gamma_multiply(0.6));
                    ui.painter().text(
                        node_pos + Vec2::new(0.0, 8.0),
                        egui::Align2::CENTER_TOP,
                        &track.title,
                        egui::FontId::new(theme.type_caption, egui::FontFamily::Proportional),
                        theme.text_secondary,
                    );
                }

                if response.dragged() || response.clicked() {
                    let pos = response.interact_pointer_pos().unwrap();
                    let target_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let target_y = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);

                    // Find nearest neighbors to set parents and bias dynamically
                    if !node_positions.is_empty() {
                        let current_cursor_pos = rect.left_top() + Vec2::new(target_x * rect.width(), (1.0 - target_y) * rect.height());
                        let mut with_dists = node_positions.iter().map(|(id, pos, _)| {
                            let d = (*pos - current_cursor_pos).length();
                            (*id, d, *pos)
                        }).collect::<Vec<_>>();
                        with_dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

                        if with_dists.len() >= 2 {
                            state.parent_a_id = Some(with_dists[0].0);
                            state.parent_b_id = Some(with_dists[1].0);
                            let d1 = with_dists[0].1;
                            let d2 = with_dists[1].1;
                            let total_d = d1 + d2;
                            if total_d > 0.001 {
                                state.transfusion_bias_x = d2 / total_d; // closer to A -> larger d2 weight -> bias_x closer to 1
                            } else {
                                state.transfusion_bias_x = 0.5;
                            }
                        } else {
                            state.parent_a_id = Some(with_dists[0].0);
                            state.parent_b_id = None;
                            state.transfusion_bias_x = 1.0;
                        }
                        state.transfusion_bias_y = target_y;
                        state.emit_dna_command(app);
                    }
                }

                // Draw lines from cursor to the 2 nearest neighbors
                let handle_pos = rect.left_top() + Vec2::new(state.transfusion_bias_x * rect.width(), (1.0 - state.transfusion_bias_y) * rect.height());
                if !node_positions.is_empty() {
                    let mut with_dists = node_positions.iter().map(|(_, pos, _)| {
                        let d = (*pos - handle_pos).length();
                        (*pos, d)
                    }).collect::<Vec<_>>();
                    with_dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                    for item in with_dists.iter().take(2) {
                        ui.painter().line_segment([handle_pos, item.0], Stroke::new(1.0, theme.accent.gamma_multiply(0.4)));
                    }
                }

                // Draw handle
                ui.painter().circle_filled(handle_pos, 8.0, theme.accent);
                ui.painter().circle_stroke(handle_pos, 8.0, Stroke::new(2.0, theme.text_primary));
            });

            ui.add_space(theme.space_md);

            // Interpolated DNA Preview (Genetic Blueprint)
            ui.vertical(|ui| {
                ui.label(RichText::new("Genetic Blueprint (Latent Space Preview)").size(theme.type_body));
                let (preview_rect, _) = ui.allocate_at_least(Vec2::new(300.0, 150.0), Sense::hover());
                ui.painter().rect_filled(preview_rect, theme.radius_md, theme.bg_inset);
                ui.painter().rect_stroke(preview_rect, theme.radius_md, theme.border_stroke);

                if let (Some(id_a), Some(id_b)) = (state.parent_a_id, state.parent_b_id) {
                    if let (Ok(Some(track_a)), Ok(Some(track_b))) = (app.library_db.get_track(id_a), app.library_db.get_track(id_b)) {
                        nullherz_dna::NeuralTransfuser::interpolate_latent(&mut state.preview_dna, &track_a.metadata.dna.spectral.latent_space, &track_b.metadata.dna.spectral.latent_space, state.transfusion_bias_x);

                        let bin_width = preview_rect.width() / 16.0;
                        let spacing = 2.0;
                        for i in 0..16 {
                            let val = state.preview_dna[i];
                            let h = val.abs().clamp(0.01, 1.0) * (preview_rect.height() / 2.0);
                            let x = preview_rect.left() + i as f32 * bin_width;

                            // Draw Bipolar Bar Chart (Stage 6 Latent Space is often centered)
                            let center_y = preview_rect.center().y;
                            let r = if val >= 0.0 {
                                egui::Rect::from_min_max(egui::pos2(x + spacing, center_y - h), egui::pos2(x + bin_width - spacing, center_y))
                            } else {
                                egui::Rect::from_min_max(egui::pos2(x + spacing, center_y), egui::pos2(x + bin_width - spacing, center_y + h))
                            };

                            let color = if i < 8 { theme.track_colors[4] } else { theme.track_colors[2] };
                            ui.painter().rect_filled(r, 1.0, color.gamma_multiply(0.8));
                        }
                        ui.painter().hline(preview_rect.x_range(), preview_rect.center().y, Stroke::new(1.0, theme.border));
                    }
                } else {
                    ui.painter().text(preview_rect.center(), egui::Align2::CENTER_CENTER, "SELECT PARENTS TO VIEW GENETIC BLUEPRINT", egui::FontId::new(theme.type_caption, egui::FontFamily::Monospace), theme.text_secondary);
                }
            });

            ui.add_space(theme.space_md);

            // Visualizers (Real-time Feedback)
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Real-time Evolution Monitor").size(theme.type_body));
                });

                Frame::none()
                    .fill(theme.bg_inset)
                    .rounding(theme.radius_md)
                    .stroke(theme.border_stroke)
                    .inner_margin(Margin::same(theme.space_sm))
                    .show(ui, |ui| {
                        if telemetry.is_some() {
                            nullherz_ui_hal::widgets::render_spectrum_analyzer(ui, &app.damped_spectrum, theme.accent, 100.0);
                        } else {
                            ui.allocate_at_least(Vec2::new(200.0, 100.0), Sense::hover());
                            ui.painter().text(ui.min_rect().center(), egui::Align2::CENTER_CENTER, "NO SIGNAL", egui::FontId::new(theme.type_body, egui::FontFamily::Proportional), theme.text_secondary);
                        }
                    });

                ui.add_space(theme.space_xs);

                Frame::none()
                    .fill(theme.bg_inset)
                    .rounding(theme.radius_md)
                    .stroke(theme.border_stroke)
                    .inner_margin(Margin::same(theme.space_sm))
                    .show(ui, |ui| {
                        if telemetry.is_some() {
                            nullherz_ui_hal::widgets::render_goniometer(ui, &app.damped_goniometer, 200.0, theme.accent);
                        } else {
                            ui.allocate_at_least(Vec2::new(200.0, 100.0), Sense::hover());
                            ui.painter().text(ui.min_rect().center(), egui::Align2::CENTER_CENTER, "GONIOMETER", egui::FontId::new(theme.type_body, egui::FontFamily::Proportional), theme.text_secondary);
                        }
                    });
            });
        });

        ui.add_space(theme.space_md);
        Frame::none()
            .fill(theme.bg_inset)
            .rounding(theme.radius_md)
            .stroke(theme.border_stroke)
            .inner_margin(Margin::same(theme.space_sm))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Spectral Bias (Weight A): {:.2}", state.transfusion_bias_x)).size(theme.type_body));
                    ui.add_space(theme.space_md);
                    ui.label(RichText::new(format!("Rhythmic Bias (Weight B): {:.2}", state.transfusion_bias_y)).size(theme.type_body));
                });
            });

        ui.add_space(theme.space_md);
        ui.horizontal(|ui| {
            let has_parents = state.parent_a_id.is_some() && state.parent_b_id.is_some();

            ui.add_enabled_ui(has_parents, |ui| {
                let btn = ui.button(RichText::new(format!("{} EVOLVE PERMANENTLY", egui_phosphor::regular::DNA)).strong().size(theme.type_label));
                if btn.clicked() {
                    if let (Some(id_a), Some(id_b)) = (state.parent_a_id, state.parent_b_id) {
                        let cmd = Command::Resource(nullherz_traits::ResourceCommand::CommitBreeding {
                            parent_a_id: id_a,
                            parent_b_id: id_b,
                            bias: state.transfusion_bias_x,
                        });
                        let _ = app.command_sender.send(cmd);
                    }
                }
            }).response.on_disabled_hover_text("Select both parents first");

            ui.add_enabled_ui(has_parents, |ui| {
                let btn = ui.button(RichText::new(format!("{} MUTATE PATTERN", egui_phosphor::regular::PIANO_KEYS)).strong().size(theme.type_label));
                if btn.clicked() {
                     if let (Some(id_a), Some(id_b)) = (state.parent_a_id, state.parent_b_id) {
                         if let (Ok(Some(track_a)), Ok(Some(track_b))) = (app.library_db.get_track(id_a), app.library_db.get_track(id_b)) {
                             let child_rhythmic = nullherz_dna::transfuse_dna(&track_a.metadata.dna, &track_b.metadata.dna, state.transfusion_bias_y).rhythmic;
                             let commands = crate::views::composer::DnaSequencer::mutate_pattern(
                                 &child_rhythmic,
                                 &app.sequencer_grid,
                                 70, // Sequencer default ID
                                 0,  // Target track 0
                                 0.2 // 20% mutation probability
                             );
                             for cmd in commands {
                                 let _ = app.command_sender.send(cmd);
                             }
                         }
                     }
                }
            }).response.on_disabled_hover_text("Select both parents first");
        });
    }

    fn emit_dna_command(&self, app: &crate::InspectorApp) {
        if let (Some(id_a), Some(id_b)) = (self.parent_a_id, self.parent_b_id) {
            if let (Ok(Some(track_a)), Ok(Some(track_b))) = (app.library_db.get_track(id_a), app.library_db.get_track(id_b)) {

                // 1. Spectral Transfusion
                let mut latent = [0.0f32; 16];
                nullherz_dna::NeuralTransfuser::interpolate_latent(&mut latent, &track_a.metadata.dna.spectral.latent_space, &track_b.metadata.dna.spectral.latent_space, self.transfusion_bias_x);

                // 2. Rhythmic Transfusion (Micro-timing)
                let mut micro_timing = [0i16; 12];
                for i in 0..12 {
                    let val_a = track_a.metadata.dna.rhythmic.micro_timing[i] as f32;
                    let val_b = track_b.metadata.dna.rhythmic.micro_timing[i] as f32;
                    micro_timing[i] = (val_a * (1.0 - self.transfusion_bias_y) + val_b * self.transfusion_bias_y) as i16;
                }

                // 3. Rhythmic Transfusion (Onset Mask)
                let mut onset_mask = [0u64; 4];
                for i in 0..4 {
                    let mask_a = track_a.metadata.dna.rhythmic.onset_mask[i];
                    let mask_b = track_b.metadata.dna.rhythmic.onset_mask[i];
                    onset_mask[i] = if self.transfusion_bias_y > 0.5 { mask_b } else { mask_a };
                }

                // Hardened: Utilizing type-safe builder to eliminate unsafe byte-packing
                let cmd = Command::Dna(DnaCommand::pack_transfusion(
                    self.target_node_idx as u64,
                    &latent,
                    &micro_timing,
                    &onset_mask
                ));

                let _ = app.command_sender.send(cmd);
            }
        }
    }
}
