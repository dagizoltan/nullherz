use egui::{Ui, Vec2, Stroke, Sense, RichText, Frame, Margin};
use nullherz_traits::{Command, DnaCommand, TransfusionWorkflowMode, ConflictResolutionMode, DonorContribution, BehaviourMap};

#[derive(Clone, PartialEq, Debug)]
pub enum BreedingMode {
    Crossover,
    Masked,
    Harmonic,
    ChaoticLogistic,
}

impl BreedingMode {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Crossover => "Domain Crossover (SLERP)",
            Self::Masked => "Invariant Masked",
            Self::Harmonic => "Harmonic Alignment",
            Self::ChaoticLogistic => "Chaotic Logistic Map",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Crossover, Self::Masked, Self::Harmonic, Self::ChaoticLogistic]
    }
}

pub struct BreederView {
    pub workflow_mode: TransfusionWorkflowMode,
    pub carrier_id: Option<u64>,
    pub donors: Vec<DonorContribution>,
    pub conflict_resolution: ConflictResolutionMode,
    pub parent_a_id: Option<u64>,
    pub parent_b_id: Option<u64>,
    pub parent_c_id: Option<u64>,
    pub parent_d_id: Option<u64>,
    pub breeding_mode: BreedingMode,
    pub transfusion_bias_x: f32, // Spectral Bias
    pub transfusion_bias_y: f32, // Rhythmic Bias
    pub selecting_parent: Option<usize>, // 0..=3
    pub preview_dna: [f32; 16],
    pub target_genre_centroid: Option<String>,
    pub _smoothed_goniometer: [f32; 128],
    pub active_behaviour_map: Option<BehaviourMap>,
    pub direct_donor_file: Option<String>,
    pub direct_carrier_file: Option<String>,
}

impl BreederView {
    pub fn new() -> Self {
        Self {
            workflow_mode: TransfusionWorkflowMode::NDonorBreeder,
            carrier_id: None,
            donors: vec![
                DonorContribution {
                    donor_id: 1,
                    donor_name: "Donor 1 (Modulator)".to_string(),
                    enable_spectral: true,
                    spectral_weight: 0.8,
                    enable_rhythmic: true,
                    rhythmic_weight: 0.5,
                    enable_transient: true,
                    transient_weight: 0.6,
                    enable_spatial: true,
                    spatial_weight: 0.5,
                    enable_pitch: false,
                    pitch_weight: 0.0,
                }
            ],
            conflict_resolution: ConflictResolutionMode::NormalizedWeightedAverage,
            parent_a_id: None,
            parent_b_id: None,
            parent_c_id: None,
            parent_d_id: None,
            breeding_mode: BreedingMode::Crossover,
            transfusion_bias_x: 0.5,
            transfusion_bias_y: 0.5,
            selecting_parent: None,
            preview_dna: [0.0; 16],
            target_genre_centroid: None,
            _smoothed_goniometer: [0.0; 128],
            active_behaviour_map: None,
            direct_donor_file: None,
            direct_carrier_file: None,
        }
    }

    pub fn show(ui: &mut Ui, state: &mut BreederView, telemetry: &Option<audio_core::Telemetry>, app: &mut crate::InspectorApp) {
        let theme = app.theme;
        ui.heading(RichText::new("DNA Breeder & Transfusion Engine").size(theme.type_heading));
        ui.add_space(theme.space_xs);

        // Workflow Mode Switcher Header
        ui.horizontal(|ui| {
            ui.label(RichText::new("WORKFLOW MODE:").size(theme.type_caption).color(theme.accent));
            ui.add_space(8.0);
            if ui.selectable_label(state.workflow_mode == TransfusionWorkflowMode::FullAudioDirect, "📁 Mode 1: Full-Audio Direct").clicked() {
                state.workflow_mode = TransfusionWorkflowMode::FullAudioDirect;
            }
            if ui.selectable_label(state.workflow_mode == TransfusionWorkflowMode::NDonorBreeder, "🧬 Mode 2: N-Donor DNA Breeding").clicked() {
                state.workflow_mode = TransfusionWorkflowMode::NDonorBreeder;
            }
            if ui.selectable_label(state.workflow_mode == TransfusionWorkflowMode::LiveDeck, "🎧 Mode 3: Live Deck Transfusion").clicked() {
                state.workflow_mode = TransfusionWorkflowMode::LiveDeck;
            }
        });
        ui.add_space(theme.space_sm);

        if let Some(parent_idx) = state.selecting_parent {
            egui::Window::new(format!("Select Parent {}", if parent_idx == 0 { "A" } else { "B" }))
                .collapsible(false).resizable(true).show(ui.ctx(), |ui| {

                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut app.library.search_query);
                    if ui.button("CLOSE").clicked() { state.selecting_parent = None; }
                });
                ui.separator();

                egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                    let mut tracks = app.library.cached_library.clone();

                    // Matchmaking Sort (Genetic Matchmaker UI)
                    let other_dna = if parent_idx == 0 {
                        state.parent_b_id.and_then(|id| app.get_cached_track(id)).map(|t| t.metadata.dna.clone())
                    } else {
                        state.parent_a_id.and_then(|id| app.get_cached_track(id)).map(|t| t.metadata.dna.clone())
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
                                match parent_idx {
                                    0 => {
                                        state.carrier_id = Some(track.id);
                                        state.parent_a_id = Some(track.id);
                                    }
                                    idx if idx > 0 => {
                                        if idx - 1 < state.donors.len() {
                                            state.donors[idx - 1].donor_id = track.id;
                                            state.donors[idx - 1].donor_name = format!("{} - {}", track.title, track.artist);
                                        }
                                        match idx {
                                            1 => state.parent_b_id = Some(track.id),
                                            2 => state.parent_c_id = Some(track.id),
                                            3 => state.parent_d_id = Some(track.id),
                                            _ => {}
                                        }
                                    }
                                    _ => {}
                                }
                                app.library.library_needs_refresh = true;
                                state.selecting_parent = None;
                            }
                        });
                    }
                });
            });
        }

        match state.workflow_mode {
            TransfusionWorkflowMode::NDonorBreeder | TransfusionWorkflowMode::LiveDeck => {
                Self::render_two_pane_breeder_ui(ui, state, theme, app, telemetry);
            }
            TransfusionWorkflowMode::FullAudioDirect => {
                Self::render_full_audio_direct_ui(ui, state, theme, app);
            }
        }
    }

    #[allow(dead_code)]
    fn render_legacy_deck_row(ui: &mut Ui, state: &mut BreederView, theme: nullherz_ui_hal::Theme, app: &mut crate::InspectorApp, telemetry: &Option<audio_core::Telemetry>) {
        // Breeding Mode & Centroid Selection Row
        ui.horizontal(|ui| {
            ui.label(RichText::new("BREEDING MODE:").strong().size(theme.type_caption).color(theme.text_secondary));
            egui::ComboBox::from_id_source("breeding_mode_combo")
                .selected_text(state.breeding_mode.name())
                .show_ui(ui, |ui| {
                    for mode in BreedingMode::all() {
                        ui.selectable_value(&mut state.breeding_mode, mode.clone(), mode.name());
                    }
                });

            ui.add_space(20.0);
            ui.label(RichText::new("TARGET CENTROID:").strong().size(theme.type_caption).color(theme.text_secondary));
            let centroids = ["(None)", "Techno / Hardgroove", "Ambient / Drone", "Drum & Bass", "House / Minimal"];
            let selected_centroid_str = state.target_genre_centroid.as_deref().unwrap_or("(None)").to_string();
            egui::ComboBox::from_id_source("genre_centroid_combo")
                .selected_text(&selected_centroid_str)
                .show_ui(ui, |ui| {
                    for centroid in centroids {
                        let is_sel = selected_centroid_str == centroid;
                        if ui.selectable_label(is_sel, centroid).clicked() {
                            state.target_genre_centroid = if centroid == "(None)" { None } else { Some(centroid.to_string()) };
                        }
                    }
                });
        });

        ui.add_space(theme.space_sm);

        // Multi-Donor Parent Selection Row (A, B, C, D) with Waveform Previews
        ui.horizontal(|ui| {
            let parent_ids = [state.parent_a_id, state.parent_b_id, state.parent_c_id, state.parent_d_id];
            let labels = ["PRIMARY CARRIER", "DONOR SLOT 1", "DONOR SLOT 2", "DONOR SLOT 3"];

            for p_idx in 0..4 {
                ui.group(|ui| {
                    ui.set_width(180.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(labels[p_idx]).size(theme.type_caption).strong().color(theme.accent));
                        let track_opt = parent_ids[p_idx].and_then(|id| app.get_cached_track(id));
                        let p_label = track_opt.as_ref()
                            .map(|t| t.title.clone())
                            .unwrap_or_else(|| "Select Donor".to_string());

                        if ui.button(RichText::new(p_label).size(theme.type_body)).clicked() {
                            state.selecting_parent = Some(p_idx);
                        }

                        // Donor Waveform Display Box
                        let (wf_rect, _) = ui.allocate_exact_size(Vec2::new(170.0, 50.0), Sense::hover());
                        ui.painter().rect_filled(wf_rect, theme.radius_sm, theme.bg_inset);
                        if let Some(ref track) = track_opt {
                            crate::views::composer::render_mini_waveform(
                                ui.painter(),
                                wf_rect.shrink(2.0),
                                &track.metadata.peaks,
                                theme.track_colors[p_idx],
                            );
                        } else {
                            ui.painter().text(
                                wf_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "NO DONOR",
                                egui::FontId::new(9.0, egui::FontFamily::Monospace),
                                theme.text_disabled,
                            );
                        }
                    });
                });

                if p_idx < 3 {
                    ui.add_space(4.0);
                    ui.label(RichText::new("×").size(theme.type_heading).strong().color(theme.accent));
                    ui.add_space(4.0);
                }
            }
        });

        ui.add_space(theme.space_lg);

        ui.horizontal(|ui| {
            // 2D Transfusion Pad (Industrial XY Pad)
            ui.vertical(|ui| {
                ui.label(RichText::new("Transfusion Pad (X: Spectral, Y: Rhythmic)").size(theme.type_body));
                let (rect, response) = ui.allocate_at_least(Vec2::splat(250.0), Sense::drag());

                ui.painter().rect_filled(rect, theme.radius_md, theme.bg_dark.linear_multiply(0.8));
                ui.painter().rect_stroke(rect, theme.radius_md, theme.border_stroke);

                // Grid lines (Industrial Look) - Decoupled from hardcoded colors
                for i in 1..4 {
                    let x = rect.left() + i as f32 * (rect.width() / 4.0);
                    ui.painter().vline(x, rect.y_range(), Stroke::new(0.5_f32, theme.border.linear_multiply(0.5)));
                    let y = rect.top() + i as f32 * (rect.height() / 4.0);
                    ui.painter().hline(rect.x_range(), y, Stroke::new(0.5_f32, theme.border.linear_multiply(0.5)));
                }

                // interact_pointer_pos is None on some drag-release frames;
                // unwrap() here could panic mid-gesture.
                if response.dragged()
                    && let Some(pos) = response.interact_pointer_pos() {
                    state.transfusion_bias_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    state.transfusion_bias_y = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);

                    state.emit_dna_command(app);
                }

                let handle_pos = rect.left_top() + Vec2::new(state.transfusion_bias_x * rect.width(), (1.0 - state.transfusion_bias_y) * rect.height());
                ui.painter().circle_filled(handle_pos, 8.0, theme.accent);
                ui.painter().circle_stroke(handle_pos, 8.0, Stroke::new(2.0_f32, theme.text_primary));
            });

            ui.add_space(theme.space_md);

            // Interpolated DNA Preview & Expected Offspring Result Waveform
            ui.vertical(|ui| {
                ui.label(RichText::new("Expected Offspring Result & Genetic Blueprint").size(theme.type_body));

                // Result Waveform Box
                let (res_wf_rect, _) = ui.allocate_exact_size(Vec2::new(300.0, 65.0), Sense::hover());
                ui.painter().rect_filled(res_wf_rect, theme.radius_md, theme.bg_inset);
                ui.painter().rect_stroke(res_wf_rect, theme.radius_md, Stroke::new(1.5_f32, theme.success));

                let track_a_opt = state.parent_a_id.and_then(|id| app.get_cached_track(id));
                let track_b_opt = state.parent_b_id.and_then(|id| app.get_cached_track(id));

                if let (Some(ta), Some(tb)) = (&track_a_opt, &track_b_opt) {
                    let mut blended_peaks = Vec::with_capacity(ta.metadata.peaks.len().max(tb.metadata.peaks.len()));
                    let max_len = ta.metadata.peaks.len().max(tb.metadata.peaks.len());
                    for i in 0..max_len {
                        let pa = ta.metadata.peaks.get(i).copied().unwrap_or(0.0);
                        let pb = tb.metadata.peaks.get(i).copied().unwrap_or(0.0);
                        let blend = pa * (1.0 - state.transfusion_bias_x) + pb * state.transfusion_bias_x;
                        blended_peaks.push(blend);
                    }
                    crate::views::composer::render_mini_waveform(
                        ui.painter(),
                        res_wf_rect.shrink(3.0),
                        &blended_peaks,
                        theme.success,
                    );
                } else {
                    ui.painter().text(
                        res_wf_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "EXPECTED OFFSPRING RESULT WAVEFORM",
                        egui::FontId::new(9.0, egui::FontFamily::Monospace),
                        theme.text_secondary,
                    );
                }

                ui.add_space(6.0);

                let (preview_rect, _) = ui.allocate_exact_size(Vec2::new(300.0, 150.0), Sense::hover());
                ui.painter().rect_filled(preview_rect, theme.radius_md, theme.bg_inset);
                ui.painter().rect_stroke(preview_rect, theme.radius_md, theme.border_stroke);

                if let (Some(track_a), Some(track_b)) = (&track_a_opt, &track_b_opt) {
                    nullherz_dna::NeuralTransfuser::interpolate_latent(&mut state.preview_dna, &track_a.metadata.dna.spectral.latent_space, &track_b.metadata.dna.spectral.latent_space, state.transfusion_bias_x);

                    let bin_width = preview_rect.width() / 16.0;
                    let spacing = 2.0;
                    for i in 0..16 {
                        let val = state.preview_dna[i];
                        let h = val.abs().clamp(0.01, 1.0) * (preview_rect.height() / 2.0);
                        let x = preview_rect.left() + i as f32 * bin_width;

                        let center_y = preview_rect.center().y;
                        let r = if val >= 0.0 {
                            egui::Rect::from_min_max(egui::pos2(x + spacing, center_y - h), egui::pos2(x + bin_width - spacing, center_y))
                        } else {
                            egui::Rect::from_min_max(egui::pos2(x + spacing, center_y), egui::pos2(x + bin_width - spacing, center_y + h))
                        };

                        let color = if i < 8 { theme.track_colors[4] } else { theme.track_colors[2] };
                        ui.painter().rect_filled(r, 1.0, color.gamma_multiply(0.8));
                    }
                    ui.painter().hline(preview_rect.x_range(), preview_rect.center().y, Stroke::new(1.0_f32, theme.border));
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
                            nullherz_ui_hal::widgets::render_spectrum_analyzer(ui, &app.viz.damped_spectrum, theme.accent, 100.0);
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
                            nullherz_ui_hal::widgets::render_goniometer(ui, &app.viz.damped_goniometer, 200.0, theme.accent);
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
                    ui.label(RichText::new(format!("Spectral Bias: {:.2}", state.transfusion_bias_x)).size(theme.type_body));
                    ui.add_space(theme.space_md);
                    ui.label(RichText::new(format!("Rhythmic Bias: {:.2}", state.transfusion_bias_y)).size(theme.type_body));
                });
            });

        ui.add_space(theme.space_md);
        ui.horizontal(|ui| {
            let has_parents = state.parent_a_id.is_some() && state.parent_b_id.is_some();

            ui.add_enabled_ui(has_parents, |ui| {
                let btn = ui.button(RichText::new(format!("{} EVOLVE PERMANENTLY", egui_phosphor::regular::DNA)).strong().size(theme.type_label));
                if btn.clicked()
                    && let (Some(id_a), Some(id_b)) = (state.parent_a_id, state.parent_b_id) {
                        let cmd = Command::Resource(nullherz_traits::ResourceCommand::CommitBreeding {
                            parent_a_id: id_a,
                            parent_b_id: id_b,
                            bias: state.transfusion_bias_x,
                        });
                        let _ = app.command_sender.send(cmd);
                    }
            }).response.on_disabled_hover_text("Select both parents first");

            ui.add_enabled_ui(has_parents, |ui| {
                let btn = ui.button(RichText::new(format!("{} MUTATE PATTERN", egui_phosphor::regular::PIANO_KEYS)).strong().size(theme.type_label));
                if btn.clicked()
                     && let (Some(id_a), Some(id_b)) = (state.parent_a_id, state.parent_b_id)
                         && let (Some(track_a), Some(track_b)) = (app.get_cached_track(id_a), app.get_cached_track(id_b)) {
                             let child_rhythmic = nullherz_dna::transfuse_dna(&track_a.metadata.dna, &track_b.metadata.dna, state.transfusion_bias_y).rhythmic;
                             // Target the FOCUSED deck's live sequencer. (This
                             // used to aim at sentinel node 70, which no node
                             // backs — the mutation commands were dropped.)
                             let grid_deck = app.decks.focused_deck.min(3);
                             if let Some(seq_node) = app.get_node_id(&format!("deck_{}_sequencer", (b'a' + grid_deck as u8) as char)) {
                                 let commands = crate::views::composer::DnaSequencer::mutate_pattern(
                                     &child_rhythmic,
                                     &app.composer.sequencer_grid[grid_deck],
                                     seq_node,
                                     0,  // Target track 0
                                     0.2 // 20% mutation probability
                                 );
                                 for cmd in commands {
                                     let _ = app.command_sender.send(cmd);
                                 }
                             }
                         }
            }).response.on_disabled_hover_text("Select both parents first");

            ui.add_enabled_ui(has_parents, |ui| {
                let btn = ui.button(RichText::new(format!("{} RHYTHMIC TRANSFUSION", egui_phosphor::regular::ARROW_RIGHT)).strong().size(theme.type_label));
                if btn.clicked()
                    && let (Some(id_a), Some(id_b)) = (state.parent_a_id, state.parent_b_id) {
                        let cmd = Command::Resource(nullherz_traits::ResourceCommand::RhythmicTransfusion {
                            source_id: id_a,
                            target_id: id_b,
                        });
                        let _ = app.command_sender.send(cmd);
                    }
            }).response.on_disabled_hover_text("Select both parents first");
        });
    }

    fn render_two_pane_breeder_ui(
        ui: &mut Ui,
        state: &mut BreederView,
        theme: nullherz_ui_hal::Theme,
        app: &mut crate::InspectorApp,
        telemetry: &Option<audio_core::Telemetry>,
    ) {
        ui.columns(2, |columns| {
            // ==================== LEFT PANE: ASSEMBLY RACK ====================
            columns[0].vertical(|ui| {
                ui.label(RichText::new("STEM DONOR ASSEMBLY RACK (SLOTS A-D)").strong().size(theme.type_heading).color(theme.accent));
                ui.add_space(theme.space_xs);

                // 4 Stem-Level Parent Donor Slots
                let slots_info = [
                    (0, "SLOT A: Rhythm / Percussion (Kick, Snare, Hats)", state.parent_a_id),
                    (1, "SLOT B: Harmonic / Bass (Sub-Bass, Synth Bass)", state.parent_b_id),
                    (2, "SLOT C: Vocal & Lead Melodies (Lead Vocal, Guitars, Keys)", state.parent_c_id),
                    (3, "SLOT D: Micro-Timing / Groove Profile", state.parent_d_id),
                ];

                for (s_idx, s_title, s_id) in slots_info {
                    Frame::none()
                        .fill(theme.bg_inset)
                        .rounding(theme.radius_md)
                        .stroke(if s_id.is_some() { Stroke::new(1.0_f32, theme.accent) } else { theme.border_stroke })
                        .inner_margin(Margin::same(theme.space_sm))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(s_title).strong().size(theme.type_caption).color(theme.track_colors[s_idx % theme.track_colors.len()]));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let track_label = s_id
                                        .and_then(|id| app.get_cached_track(id))
                                        .map(|t| format!("{} - {}", t.title, t.artist))
                                        .unwrap_or_else(|| "Select Stem Donor...".to_string());
                                    if ui.button(RichText::new(track_label).strong().size(theme.type_body)).clicked() {
                                        state.selecting_parent = Some(s_idx);
                                    }
                                });
                            });
                        });
                    ui.add_space(theme.space_xs);
                }

                ui.add_space(theme.space_sm);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("DYNAMIC DONOR STACK").strong().size(theme.type_caption).color(theme.text_secondary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("+ ADD DONOR SLOT").strong().color(theme.accent)).clicked() {
                            let id = (state.donors.len() + 1) as u64;
                            state.donors.push(DonorContribution {
                                donor_id: id,
                                donor_name: format!("Donor Slot {}", id),
                                enable_spectral: true,
                                spectral_weight: 0.5,
                                enable_rhythmic: true,
                                rhythmic_weight: 0.5,
                                enable_transient: true,
                                transient_weight: 0.5,
                                enable_spatial: true,
                                spatial_weight: 0.5,
                                enable_pitch: false,
                                pitch_weight: 0.0,
                            });
                        }
                    });
                });

                ui.add_space(theme.space_xs);

                // Stackable Donor Cards
                let mut remove_idx = None;
                egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                    for (idx, donor) in state.donors.iter_mut().enumerate() {
                        Frame::none()
                            .fill(theme.bg_inset)
                            .rounding(theme.radius_md)
                            .stroke(theme.border_stroke)
                            .inner_margin(Margin::same(theme.space_sm))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let slot_title = format!("DONOR SLOT {}", idx + 1);
                                    ui.label(RichText::new(&slot_title).strong().size(theme.type_label).color(theme.track_colors[idx % theme.track_colors.len()]));
                                    ui.add_space(8.0);
                                    let donor_track_label = app.get_cached_track(donor.donor_id)
                                        .map(|t| format!("{} - {}", t.title, t.artist))
                                        .unwrap_or_else(|| "Select Track...".to_string());
                                    if ui.button(RichText::new(donor_track_label).size(theme.type_body)).clicked() {
                                        state.selecting_parent = Some(idx + 1);
                                    }

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.button(RichText::new("X").color(theme.danger)).clicked() {
                                            remove_idx = Some(idx);
                                        }
                                    });
                                });

                                ui.add_space(6.0);

                                // Compact Trait Pills / Chips Row
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("TRAITS:").size(theme.type_caption).color(theme.text_secondary));
                                    ui.toggle_value(&mut donor.enable_spectral, "🎛️ Timbre");
                                    ui.toggle_value(&mut donor.enable_rhythmic, "🥁 Groove");
                                    ui.toggle_value(&mut donor.enable_transient, "⚡ Attack");
                                    ui.toggle_value(&mut donor.enable_spatial, "🌌 Width");
                                    ui.toggle_value(&mut donor.enable_pitch, "💜 Pitch");
                                });

                                ui.separator();

                                // Weight Sliders for Selected Traits
                                ui.horizontal(|ui| {
                                    if donor.enable_spectral {
                                        ui.label(RichText::new("Timbre:").size(theme.type_caption));
                                        ui.add(egui::Slider::new(&mut donor.spectral_weight, 0.0..=1.0).text(""));
                                    }
                                    if donor.enable_rhythmic {
                                        ui.label(RichText::new("Groove:").size(theme.type_caption));
                                        ui.add(egui::Slider::new(&mut donor.rhythmic_weight, 0.0..=1.0).text(""));
                                    }
                                    if donor.enable_transient {
                                        ui.label(RichText::new("Attack:").size(theme.type_caption));
                                        ui.add(egui::Slider::new(&mut donor.transient_weight, 0.0..=1.0).text(""));
                                    }
                                    if donor.enable_spatial {
                                        ui.label(RichText::new("Width:").size(theme.type_caption));
                                        ui.add(egui::Slider::new(&mut donor.spatial_weight, 0.0..=1.0).text(""));
                                    }
                                    if donor.enable_pitch {
                                        ui.label(RichText::new("Pitch:").size(theme.type_caption));
                                        ui.add(egui::Slider::new(&mut donor.pitch_weight, 0.0..=1.0).text(""));
                                    }
                                });
                            });
                        ui.add_space(theme.space_xs);
                    }
                });

                if let Some(idx) = remove_idx {
                    if state.donors.len() > 1 {
                        state.donors.remove(idx);
                    }
                }
            });

            // ==================== RIGHT PANE: BLUEPRINT & ACTION ====================
            columns[1].vertical(|ui| {
                ui.label(RichText::new("OFFSPRING BLUEPRINT & ACTION").strong().size(theme.type_heading).color(theme.accent));
                ui.add_space(theme.space_xs);

                // Expected Offspring Waveform Preview
                ui.label(RichText::new("🌊 EXPECTED OFFSPRING RESULT WAVEFORM").size(theme.type_caption).strong().color(theme.text_secondary));
                let (res_wf_rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 70.0), Sense::hover());
                ui.painter().rect_filled(res_wf_rect, theme.radius_md, theme.bg_inset);
                ui.painter().rect_stroke(res_wf_rect, theme.radius_md, Stroke::new(1.5_f32, theme.success));

                let carrier_track_opt = state.carrier_id.or(state.parent_a_id).and_then(|id| app.get_cached_track(id));
                let donor_track_opt = state.donors.first().and_then(|d| app.get_cached_track(d.donor_id));

                if let (Some(ta), Some(tb)) = (&carrier_track_opt, &donor_track_opt) {
                    let mut blended_peaks = Vec::with_capacity(ta.metadata.peaks.len().max(tb.metadata.peaks.len()));
                    let max_len = ta.metadata.peaks.len().max(tb.metadata.peaks.len());
                    for i in 0..max_len {
                        let pa = ta.metadata.peaks.get(i).copied().unwrap_or(0.0);
                        let pb = tb.metadata.peaks.get(i).copied().unwrap_or(0.0);
                        let blend = pa * 0.5 + pb * 0.5;
                        blended_peaks.push(blend);
                    }
                    crate::views::composer::render_mini_waveform(
                        ui.painter(),
                        res_wf_rect.shrink(3.0),
                        &blended_peaks,
                        theme.success,
                    );
                } else {
                    ui.painter().text(
                        res_wf_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "SELECT CARRIER AND DONORS TO PREVIEW RESULT",
                        egui::FontId::new(9.0, egui::FontFamily::Monospace),
                        theme.text_secondary,
                    );
                }

                ui.add_space(theme.space_sm);

                // 16-D Latent Genetic Blueprint
                ui.label(RichText::new("📊 16-D LATENT GENETIC BLUEPRINT").size(theme.type_caption).strong().color(theme.text_secondary));
                let (preview_rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 120.0), Sense::hover());
                ui.painter().rect_filled(preview_rect, theme.radius_md, theme.bg_inset);
                ui.painter().rect_stroke(preview_rect, theme.radius_md, theme.border_stroke);

                if let (Some(track_a), Some(track_b)) = (&carrier_track_opt, &donor_track_opt) {
                    nullherz_dna::NeuralTransfuser::interpolate_latent(&mut state.preview_dna, &track_a.metadata.dna.spectral.latent_space, &track_b.metadata.dna.spectral.latent_space, state.transfusion_bias_x);

                    let bin_width = preview_rect.width() / 16.0;
                    let spacing = 2.0;
                    for i in 0..16 {
                        let val = state.preview_dna[i];
                        let h = val.abs().clamp(0.01, 1.0) * (preview_rect.height() / 2.0);
                        let x = preview_rect.left() + i as f32 * bin_width;

                        let center_y = preview_rect.center().y;
                        let r = if val >= 0.0 {
                            egui::Rect::from_min_max(egui::pos2(x + spacing, center_y - h), egui::pos2(x + bin_width - spacing, center_y))
                        } else {
                            egui::Rect::from_min_max(egui::pos2(x + spacing, center_y), egui::pos2(x + bin_width - spacing, center_y + h))
                        };

                        let color = if i < 8 { theme.track_colors[4] } else { theme.track_colors[2] };
                        ui.painter().rect_filled(r, 1.0, color.gamma_multiply(0.8));
                    }
                    ui.painter().hline(preview_rect.x_range(), preview_rect.center().y, Stroke::new(1.0_f32, theme.border));
                } else {
                    ui.painter().text(preview_rect.center(), egui::Align2::CENTER_CENTER, "GENETIC BLUEPRINT PREVIEW", egui::FontId::new(theme.type_caption, egui::FontFamily::Monospace), theme.text_secondary);
                }

                ui.add_space(theme.space_sm);

                // Real-time Evolution Monitor Spectrum Analyzer Widget
                ui.add_space(theme.space_sm);
                ui.label(RichText::new("REAL-TIME EVOLUTION SPECTRUM").size(theme.type_caption).strong().color(theme.text_secondary));
                Frame::none()
                    .fill(theme.bg_inset)
                    .rounding(theme.radius_md)
                    .stroke(theme.border_stroke)
                    .inner_margin(Margin::same(theme.space_xs))
                    .show(ui, |ui| {
                        if telemetry.is_some() {
                            nullherz_ui_hal::widgets::render_spectrum_analyzer(ui, &app.viz.damped_spectrum, theme.accent, 80.0);
                        } else {
                            ui.allocate_at_least(Vec2::new(ui.available_width(), 80.0), Sense::hover());
                            ui.painter().text(ui.min_rect().center(), egui::Align2::CENTER_CENTER, "REAL-TIME MONITOR ACTIVE", egui::FontId::new(theme.type_caption, egui::FontFamily::Proportional), theme.text_secondary);
                        }
                    });

                // Conflict Resolution Picker
                ui.horizontal(|ui| {
                    ui.label(RichText::new("CONFLICT RESOLUTION:").strong().size(theme.type_caption).color(theme.text_secondary));
                    let mode_str = match state.conflict_resolution {
                        ConflictResolutionMode::NormalizedWeightedAverage => "Normalized Weighted Average",
                        ConflictResolutionMode::PriorityOverride => "Priority Override",
                        ConflictResolutionMode::MorphSweep => "Morph Sweep",
                    };

                    egui::ComboBox::from_id_source("conflict_res_combo_split")
                        .selected_text(mode_str)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.conflict_resolution, ConflictResolutionMode::NormalizedWeightedAverage, "Normalized Weighted Average");
                            ui.selectable_value(&mut state.conflict_resolution, ConflictResolutionMode::PriorityOverride, "Priority Override");
                            ui.selectable_value(&mut state.conflict_resolution, ConflictResolutionMode::MorphSweep, "Morph Sweep");
                        });
                });

                ui.add_space(theme.space_md);

                // Action Evolution Trigger
                let has_carrier = state.carrier_id.or(state.parent_a_id).is_some();
                ui.add_enabled_ui(has_carrier, |ui| {
                    let btn = ui.button(
                        RichText::new(format!("⚡ BAKE & EVOLVE PERMANENT TRACK {}", egui_phosphor::regular::DNA))
                            .strong()
                            .size(theme.type_heading)
                            .color(theme.bg_dark)
                    );
                    if btn.clicked() {
                        state.emit_dna_command(app);
                    }
                }).response.on_disabled_hover_text("Select Carrier track first");
            });
        });
    }

    fn render_full_audio_direct_ui(ui: &mut Ui, state: &mut BreederView, theme: nullherz_ui_hal::Theme, app: &mut crate::InspectorApp) {
        ui.group(|ui| {
            ui.label(RichText::new("FULL-AUDIO DIRECT TRANSFUSION ENGINE").strong().size(theme.type_heading).color(theme.accent));
            ui.add_space(theme.space_xs);
            ui.label(RichText::new("Direct file-based behavior map extraction and cross-synthesis on full WAV/FLAC audio files.").size(theme.type_caption).color(theme.text_secondary));

            ui.add_space(theme.space_md);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("CARRIER AUDIO FILE:").strong().size(theme.type_caption));
                    let carrier_file = state.direct_carrier_file.as_deref().unwrap_or("[No Carrier File Selected]");
                    ui.label(RichText::new(carrier_file).size(theme.type_body));
                    if ui.button("Select Carrier File...").clicked() {
                        state.direct_carrier_file = Some("sample_carrier_stem.wav".to_string());
                    }
                });

                ui.add_space(40.0);

                ui.vertical(|ui| {
                    ui.label(RichText::new("DONOR AUDIO FILE:").strong().size(theme.type_caption));
                    let donor_file = state.direct_donor_file.as_deref().unwrap_or("[No Donor File Selected]");
                    ui.label(RichText::new(donor_file).size(theme.type_body));
                    if ui.button("Select Donor File...").clicked() {
                        state.direct_donor_file = Some("sample_donor_stem.wav".to_string());
                    }
                });
            });

            ui.add_space(theme.space_md);

            // Behaviour Map Contour Inspection
            ui.label(RichText::new("EXTRACTED BEHAVIOUR MAP CONTOURS").strong().size(theme.type_label).color(theme.text_secondary));
            let (map_rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 100.0), Sense::hover());
            ui.painter().rect_filled(map_rect, theme.radius_md, theme.bg_inset);
            ui.painter().rect_stroke(map_rect, theme.radius_md, theme.border_stroke);

            // Render simulated 5 behavior contour curves
            let w = map_rect.width();
            let h = map_rect.height();
            let pts_count = 64;
            for i in 0..pts_count - 1 {
                let x1 = map_rect.left() + (i as f32 / pts_count as f32) * w;
                let x2 = map_rect.left() + ((i + 1) as f32 / pts_count as f32) * w;

                let y1_energy = map_rect.bottom() - (0.3 + 0.5 * (i as f32 * 0.2).sin().abs()) * h;
                let y2_energy = map_rect.bottom() - (0.3 + 0.5 * ((i + 1) as f32 * 0.2).sin().abs()) * h;
                ui.painter().line_segment([egui::pos2(x1, y1_energy), egui::pos2(x2, y2_energy)], Stroke::new(1.5_f32, theme.accent));

                let y1_transient = map_rect.bottom() - (0.1 + 0.8 * (i % 8 == 0) as u8 as f32) * h;
                let y2_transient = map_rect.bottom() - (0.1 + 0.8 * ((i + 1) % 8 == 0) as u8 as f32) * h;
                ui.painter().line_segment([egui::pos2(x1, y1_transient), egui::pos2(x2, y2_transient)], Stroke::new(1.5_f32, theme.danger));
            }

            ui.add_space(theme.space_md);

            ui.horizontal(|ui| {
                if ui.button(RichText::new("💾 Save Behaviour Preset (.behaviourmap)").strong()).clicked() {
                    state.active_behaviour_map = Some(BehaviourMap {
                        name: "Extracted Preset".to_string(),
                        duration_sec: 16.0,
                        energy_envelope: vec![0.5; 64],
                        motion_trajectory: vec![0.5; 64],
                        transient_spikes: vec![0.0; 64],
                        texture_density: vec![0.5; 64],
                        pitch_contour: vec![0.5; 64],
                    });
                }

                ui.add_space(20.0);

                if ui.button(RichText::new("⚡ BAKE & RENDER TRANSFUSED WAV").strong().color(theme.success)).clicked() {
                    let cmd = Command::Resource(nullherz_traits::ResourceCommand::OfflineRenderTransfusion {
                        carrier_id: state.carrier_id.unwrap_or(0),
                        donor_id: state.donors.first().map(|d| d.donor_id).unwrap_or(0),
                    });
                    let _ = app.command_sender.send(cmd);
                }
            });
        });
    }

    fn emit_dna_command(&self, app: &crate::InspectorApp) {
        let target_node = app.topo.node_map.get("personality_inheritance")
            .or_else(|| app.topo.node_map.get("master_personality"))
            .copied()
            .unwrap_or(0);

        if self.workflow_mode == TransfusionWorkflowMode::NDonorBreeder {
            if let Some(c_id) = self.carrier_id.or(self.parent_a_id)
                && let Some(carrier_track) = app.get_cached_track(c_id) {

                    let mut donor_tuples = Vec::new();
                    for donor_contrib in &self.donors {
                        if let Some(donor_track) = app.get_cached_track(donor_contrib.donor_id) {
                            donor_tuples.push((donor_track.metadata.dna.clone(), donor_contrib.clone()));
                        }
                    }

                    let multi_child = nullherz_dna::transfuse_multi_donor(
                        &carrier_track.metadata.dna,
                        &donor_tuples,
                        self.conflict_resolution,
                    );

                    let cmd = Command::Dna(DnaCommand::pack_transfusion(
                        target_node as u64,
                        &multi_child.spectral.latent_space,
                        &multi_child.rhythmic.micro_timing,
                        &multi_child.rhythmic.onset_mask,
                    ));

                    let _ = app.command_sender.send(cmd);
                }
            return;
        }

        if let (Some(id_a), Some(id_b)) = (self.parent_a_id, self.parent_b_id)
            && let (Some(track_a), Some(track_b)) = (app.get_cached_track(id_a), app.get_cached_track(id_b)) {

                // 1. Spectral Transfusion
                let mut latent = [0.0f32; 16];
                nullherz_dna::NeuralTransfuser::interpolate_latent(&mut latent, &track_a.metadata.dna.spectral.latent_space, &track_b.metadata.dna.spectral.latent_space, self.transfusion_bias_x);

                // 2. Rhythmic Transfusion (Micro-timing)
                let mut micro_timing = [0i16; 12];
                for (i, item) in micro_timing.iter_mut().enumerate() {
                    let val_a = track_a.metadata.dna.rhythmic.micro_timing[i] as f32;
                    let val_b = track_b.metadata.dna.rhythmic.micro_timing[i] as f32;
                    *item = (val_a * (1.0 - self.transfusion_bias_y) + val_b * self.transfusion_bias_y) as i16;
                }

                // 3. Rhythmic Transfusion (Onset Mask)
                let mut onset_mask = [0u64; 4];
                for (i, item) in onset_mask.iter_mut().enumerate() {
                    let mask_a = track_a.metadata.dna.rhythmic.onset_mask[i];
                    let mask_b = track_b.metadata.dna.rhythmic.onset_mask[i];
                    *item = if self.transfusion_bias_y > 0.5 { mask_b } else { mask_a };
                }

                let cmd = Command::Dna(DnaCommand::pack_transfusion(
                    target_node as u64,
                    &latent,
                    &micro_timing,
                    &onset_mask
                ));

                let _ = app.command_sender.send(cmd);
            }
    }
}
