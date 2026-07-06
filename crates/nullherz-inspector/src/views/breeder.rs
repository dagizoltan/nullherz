use egui::{Ui, Vec2, Color32, Stroke, Sense, RichText};
use nullherz_traits::{Command, DnaCommand};

pub struct BreederView {
    pub parent_a_id: Option<u64>,
    pub parent_b_id: Option<u64>,
    pub transfusion_bias_x: f32, // Spectral Bias
    pub transfusion_bias_y: f32, // Rhythmic Bias
    pub target_node_idx: u32,
    pub selecting_parent: Option<usize>, // 0 for A, 1 for B
    pub preview_dna: [f32; 16],
    pub smoothed_goniometer: [f32; 128],
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
            smoothed_goniometer: [0.0; 128],
        }
    }

    pub fn show(ui: &mut Ui, state: &mut BreederView, telemetry: &Option<audio_core::Telemetry>, app: &mut crate::InspectorApp) {
        ui.heading("DNA Breeder");
        ui.add_space(10.0);

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
                        state.parent_b_id.and_then(|id| app.library_db.get_track(id).ok().flatten()).map(|t| t.metadata.dna)
                    } else {
                        state.parent_a_id.and_then(|id| app.library_db.get_track(id).ok().flatten()).map(|t| t.metadata.dna)
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
                                let color = if s > 0.8 { Color32::from_rgb(0, 255, 200) } else if s > 0.5 { Color32::YELLOW } else { Color32::GRAY };
                                ui.add(egui::ProgressBar::new(s).desired_width(60.0).fill(color).text(format!("{:.0}%", s * 100.0)));
                            }

                            let label = format!("{} - {}", track.title, track.artist);
                            if ui.selectable_label(false, label).clicked() {
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
                ui.label("Parent A");
                let label = state.parent_a_id.and_then(|id| app.library_db.get_track(id).ok().flatten())
                    .map(|t| t.title).unwrap_or_else(|| "Select Sample".to_string());

                if ui.button(label).clicked() {
                    state.selecting_parent = Some(0);
                }
            });

            ui.add_space(40.0);
            ui.label(RichText::new("X").size(20.0).strong());
            ui.add_space(40.0);

            // Parent B Selection
            ui.vertical(|ui| {
                ui.label("Parent B");
                let label = state.parent_b_id.and_then(|id| app.library_db.get_track(id).ok().flatten())
                    .map(|t| t.title).unwrap_or_else(|| "Select Sample".to_string());

                if ui.button(label).clicked() {
                    state.selecting_parent = Some(1);
                }
            });
        });

        ui.add_space(30.0);

        ui.horizontal(|ui| {
            // 2D Transfusion Pad
            ui.vertical(|ui| {
                ui.label("Transfusion Pad (X: Spectral, Y: Rhythmic)");
                let (rect, response) = ui.allocate_at_least(Vec2::splat(250.0), Sense::drag());

                ui.painter().rect_filled(rect, 4.0, Color32::from_black_alpha(150));
                ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_rgb(0, 255, 200)));

                // Grid lines
                ui.painter().line_segment([rect.center_top(), rect.center_bottom()], Stroke::new(0.5, Color32::DARK_GRAY));
                ui.painter().line_segment([rect.left_center(), rect.right_center()], Stroke::new(0.5, Color32::DARK_GRAY));

                if response.dragged() {
                    let pos = response.interact_pointer_pos().unwrap();
                    state.transfusion_bias_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    state.transfusion_bias_y = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);

                    state.emit_dna_command(app);
                }

                let handle_pos = rect.left_top() + Vec2::new(state.transfusion_bias_x * rect.width(), (1.0 - state.transfusion_bias_y) * rect.height());
                ui.painter().circle_filled(handle_pos, 8.0, Color32::from_rgb(0, 255, 200));
                ui.painter().circle_stroke(handle_pos, 8.0, Stroke::new(2.0, Color32::WHITE));
            });

            ui.add_space(20.0);

            // Interpolated DNA Preview (Genetic Blueprint)
            ui.vertical(|ui| {
                ui.label("Genetic Blueprint (Interpolated DNA)");
                let (preview_rect, _) = ui.allocate_at_least(Vec2::new(250.0, 100.0), Sense::hover());
                ui.painter().rect_filled(preview_rect, 2.0, Color32::from_black_alpha(100));

                if let (Some(id_a), Some(id_b)) = (state.parent_a_id, state.parent_b_id) {
                    if let (Ok(Some(track_a)), Ok(Some(track_b))) = (app.library_db.get_track(id_a), app.library_db.get_track(id_b)) {
                        nullherz_dna::NeuralTransfuser::interpolate_latent(&mut state.preview_dna, &track_a.metadata.dna.spectral.latent_space, &track_b.metadata.dna.spectral.latent_space, state.transfusion_bias_x);

                        let bin_width = preview_rect.width() / 16.0;
                        for i in 0..16 {
                            let h = state.preview_dna[i].clamp(0.0, 1.0) * preview_rect.height();
                            let x = preview_rect.left() + i as f32 * bin_width;
                            let r = egui::Rect::from_min_max(
                                egui::pos2(x, preview_rect.bottom() - h),
                                egui::pos2(x + bin_width - 1.0, preview_rect.bottom())
                            );
                            ui.painter().rect_filled(r, 0.0, Color32::from_rgb(0, 255, 200));
                        }
                    }
                } else {
                    ui.painter().text(preview_rect.center(), egui::Align2::CENTER_CENTER, "SELECT PARENTS FOR PREVIEW", egui::FontId::proportional(12.0), Color32::GRAY);
                }
            });

            ui.add_space(20.0);

            // Visualizers (Real-time Feedback)
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Real-time Evolution Monitor");
                });

                ui.group(|ui| {
                    if telemetry.is_some() {
                        crate::widgets::render_spectrum_analyzer(ui, &app.damped_spectrum, Color32::from_rgb(0, 255, 200), 100.0);
                    } else {
                        ui.allocate_at_least(Vec2::new(200.0, 100.0), Sense::hover());
                        ui.painter().text(ui.min_rect().center(), egui::Align2::CENTER_CENTER, "NO SIGNAL", egui::FontId::proportional(12.0), Color32::GRAY);
                    }
                });

                ui.add_space(10.0);

                ui.group(|ui| {
                    if let Some(t) = &*telemetry {
                        let alpha = app.visualizer_damping.clamp(0.01, 1.0);
                        let decay = alpha * 0.5;
                        for i in 0..128 {
                            let target = t.goniometer_pts[i];
                            let a = if target.abs() > state.smoothed_goniometer[i].abs() { alpha } else { decay };
                            state.smoothed_goniometer[i] = state.smoothed_goniometer[i] * (1.0 - a) + target * a;
                        }
                        crate::widgets::render_goniometer(ui, &state.smoothed_goniometer, 200.0, Color32::from_rgb(0, 255, 200));
                    } else {
                        ui.allocate_at_least(Vec2::new(200.0, 100.0), Sense::hover());
                        ui.painter().text(ui.min_rect().center(), egui::Align2::CENTER_CENTER, "GONIOMETER", egui::FontId::proportional(12.0), Color32::GRAY);
                    }
                });
            });
        });

        ui.add_space(20.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Spectral Bias: {:.2}", state.transfusion_bias_x));
                ui.add_space(20.0);
                ui.label(format!("Rhythmic Bias: {:.2}", state.transfusion_bias_y));
            });
        });

        ui.add_space(20.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("🧬 EVOLVE PERMANENTLY").strong().size(16.0)).clicked() {
                if let (Some(id_a), Some(id_b)) = (state.parent_a_id, state.parent_b_id) {
                    let cmd = Command::Resource(nullherz_traits::ResourceCommand::CommitBreeding {
                        parent_a_id: id_a,
                        parent_b_id: id_b,
                        bias: state.transfusion_bias_x,
                    });
                    let _ = app.command_sender.send(cmd);
                }
            }

            if ui.button(RichText::new("🎹 MUTATE PATTERN").strong().size(16.0)).clicked() {
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
