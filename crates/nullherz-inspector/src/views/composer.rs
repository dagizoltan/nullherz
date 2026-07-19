use egui::{Ui, ScrollArea, Vec2, Sense, RichText, Stroke, Frame, Rounding, Margin};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;
use nullherz_traits::{Command, PerformanceCommand, CoreCommand, MixerCommand};
pub use nullherz_conductor::pattern_manager::DnaSequencer;

/// Helper to determine step status from telemetry safely.
/// Returns (is_playing, is_starting).
pub fn check_step_telemetry(
    _telemetry: &Option<Telemetry>,
    _track_idx: usize,
    _slot_idx: usize,
) -> (bool, bool) {
    // Both are false because clip-slot based fields (active_clips and starting_clips_mask)
    // are not step-sequencer compatible.
    (false, false)
}

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    // The composer edits the FOCUSED deck's sequencer node, resolved by
    // name. (It used to target "sequencer_node", which never existed — the
    // unwrap_or(0) fallback sent every step/mute/solo command to node 0,
    // deck A's SAMPLER. The composer has never actually driven a sequencer.)
    let Some(seq_node) = app.get_node_id(&format!(
        "deck_{}_sequencer",
        (b'a' + app.decks.focused_deck.min(3) as u8) as char
    )) else {
        ui.label("Sequencer not available yet (topology still installing).");
        return;
    };
    let grid_deck = app.decks.focused_deck.min(3);
    ui.horizontal(|ui| {
        ui.heading(RichText::new("SESSION VIEW (COMPOSER)").strong().color(app.theme.text_primary));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
             ui.label(egui::RichText::new("QUANTIZED: 1 BAR").color(app.theme.accent).size(app.theme.type_caption));
        });
    });
    ui.add_space(app.theme.space_sm);

    // Global Transport & Master Controls
    ui.horizontal(|ui| {
        // Global PLAY / STOP
        let play_btn = ui.selectable_label(app.decks.global_playing, RichText::new("▶ PLAY").strong().color(if app.decks.global_playing { app.theme.success } else { app.theme.text_secondary }));
        if play_btn.clicked() {
            app.decks.global_playing = true;
            let _ = app.command_sender.send(Command::Core(CoreCommand::Play));
        }

        let stop_btn = ui.selectable_label(!app.decks.global_playing, RichText::new("■ STOP").strong().color(if !app.decks.global_playing { app.theme.danger } else { app.theme.text_secondary }));
        if stop_btn.clicked() {
            app.decks.global_playing = false;
            let _ = app.command_sender.send(Command::Core(CoreCommand::Stop));
        }

        ui.add_space(app.theme.space_md);

        // Global BPM
        ui.label(RichText::new("BPM").strong().size(app.theme.type_caption).color(app.theme.text_secondary));
        let mut bpm = app.decks.global_bpm;
        if ui.add(egui::DragValue::new(&mut bpm).speed(0.1).clamp_range(20.0..=300.0)).changed() {
            app.decks.global_bpm = bpm;
            let _ = app.command_sender.send(Command::Core(CoreCommand::SetBpm(bpm)));
        }

        ui.add_space(app.theme.space_md);

        let is_recording = app.composer.record_automation;
        ui.toggle_value(&mut app.composer.record_automation, RichText::new("🔴 RECORD AUTOMATION").color(if is_recording { app.theme.danger } else { app.theme.text_secondary }));
        ui.add_space(app.theme.space_md);

        if ui.button("STOP ALL CLIPS").clicked() {
            for i in 0..16 {
                 let _ = app.command_sender.send(Command::Performance(PerformanceCommand::ClearTrackPattern { node_idx: seq_node, track_idx: i as u32 }));
                 app.composer.sequencer_grid[grid_deck][i].fill(0.0);
            }
        }

        ui.add_space(app.theme.space_md);
        ui.label(RichText::new("MASTER VOL").size(app.theme.type_caption).color(app.theme.text_secondary));
        widgets::render_horizontal_fader(ui, &mut app.mixer.master_gain, 0.0..=1.5, app.theme.text_primary, 100.0, 12.0);
    });
    ui.add_space(app.theme.space_sm);

    // Global Scene Launchers Control Row
    ui.horizontal(|ui| {
        ui.label(RichText::new("LAUNCH SCENE:").strong().size(app.theme.type_caption).color(app.theme.text_secondary));
        for scene_idx in 0..8 {
            let btn_text = format!("SCENE {}", scene_idx + 1);
            if ui.add_sized([70.0, 20.0], egui::Button::new(RichText::new(btn_text).size(app.theme.type_caption).strong()).fill(app.theme.accent.linear_multiply(0.12))).clicked() {
                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::LaunchClip { row: 0xFF, col: scene_idx as u32 }));
            }
        }
    });
    ui.add_space(app.theme.space_sm);

    // Continuous surface frame wrapping stationary headers + scrollable endless grid
    Frame::none()
        .fill(app.theme.bg_dark)
        .stroke(app.theme.border_stroke)
        .rounding(Rounding::same(app.theme.radius_md))
        .inner_margin(Margin::same(app.theme.space_sm))
        .show(ui, |ui| {
            let mut extend_grid = false;
            let steps_count = app.composer.sequencer_grid[grid_deck][0].len();

            ui.horizontal(|ui| {
                // 1. LEFT SIDE: Stationary Track Headers column (100.0px width)
                ui.vertical(|ui| {
                    // Step number header space on the left to align with grid step numbers
                    ui.add_space(20.0);
                    ui.add_space(app.theme.space_sm);

                    for track_idx in 0..16 {
                        let track_color = app.theme.track_colors[track_idx];
                        let is_muted = app.composer.track_mutes[track_idx];
                        let is_selected = app.composer.selected_composer_track == Some(track_idx);

                        let header_bg = if is_selected {
                            track_color.gamma_multiply(0.4)
                        } else if is_muted {
                            app.theme.bg_inset
                        } else {
                            track_color.gamma_multiply(0.2)
                        };

                        let inner_resp = Frame::none()
                            .fill(header_bg)
                            .rounding(Rounding::same(app.theme.radius_sm))
                            .inner_margin(Margin::symmetric(app.theme.space_xs, app.theme.space_xs))
                            .show(ui, |ui| {
                                ui.set_width(90.0);
                                ui.set_height(22.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(app.theme.space_xs);
                                    // 8x8px color swatch/chip
                                    let (swatch_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
                                    ui.painter().rect_filled(swatch_rect, Rounding::same(1.5), track_color);
                                    ui.add_space(app.theme.space_xs);
                                    ui.label(RichText::new(format!("TRK {}", track_idx + 1)).strong().size(app.theme.type_body).color(app.theme.text_primary));
                                });
                            });

                        let rect = inner_resp.response.rect;

                        // Make the header responsive to click to select/expand accordion
                        let response = ui.interact(rect, ui.make_persistent_id(format!("trk_hdr_{}", track_idx)), Sense::click());
                        if response.clicked() {
                            if is_selected {
                                app.composer.selected_composer_track = None;
                            } else {
                                app.composer.selected_composer_track = Some(track_idx);
                            }
                        }

                        // Accordion expansion in the Left Stationary side
                        if is_selected {
                            ui.add_space(4.0);
                            Frame::none()
                                .fill(app.theme.bg_inset)
                                .rounding(Rounding::same(app.theme.radius_sm))
                                .stroke(app.theme.border_stroke)
                                .inner_margin(Margin::same(4.0))
                                .show(ui, |ui| {
                                    ui.set_width(90.0);
                                    ui.set_height(80.0);
                                    ui.vertical_centered(|ui| {
                                        ui.horizontal(|ui| {
                                            // Activator ON/OFF Mute
                                            let activator_color = if !is_muted { app.theme.warning } else { app.theme.bg_inset };
                                            if ui.add_sized([22.0, 18.0], egui::Button::new(RichText::new("ON").size(app.theme.type_caption).strong()).fill(activator_color)).clicked() {
                                                app.composer.track_mutes[track_idx] = !is_muted;
                                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetTrackMute { node_idx: seq_node, track_idx: track_idx as u32, muted: app.composer.track_mutes[track_idx] }));
                                            }

                                            // Solo Button
                                            let is_soloed = app.composer.track_solos[track_idx];
                                            let solo_color = if is_soloed { app.theme.track_colors[1] } else { app.theme.bg_inset };
                                            if ui.add_sized([18.0, 18.0], egui::Button::new(RichText::new("S").size(app.theme.type_caption).strong()).fill(solo_color)).clicked() {
                                                app.composer.track_solos[track_idx] = !is_soloed;
                                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetTrackSolo { node_idx: seq_node, track_idx: track_idx as u32, soloed: app.composer.track_solos[track_idx] }));
                                            }

                                            // Stop Clip button (compact)
                                            let stop_btn = egui::Button::new(RichText::new("■").size(app.theme.type_caption).strong()).fill(app.theme.bg_inset);
                                            if ui.add_sized([18.0, 18.0], stop_btn).on_hover_text("Stop clip").clicked() {
                                                app.composer.sequencer_grid[grid_deck][track_idx].fill(0.0);
                                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::ClearTrackPattern { node_idx: seq_node, track_idx: track_idx as u32 }));
                                            }
                                        });

                                        ui.add_space(4.0);

                                        // Sequencer routing target dropdown
                                        let current_target = app.composer.track_targets[track_idx].clone();
                                        let mut sorted_nodes = app.node_names();
                                        sorted_nodes.sort_by(|a, b| a.0.cmp(&b.0));

                                        let mut changed = false;
                                        let mut selected_name = current_target.clone();
                                        let mut selected_node_idx = 0u32;

                                        egui::ComboBox::from_id_source(format!("seq_tgt_{}", track_idx))
                                            .width(80.0)
                                            .selected_text(&current_target)
                                            .show_ui(ui, |ui| {
                                                for (name, node_idx) in sorted_nodes {
                                                    if ui.selectable_label(current_target == name, &name).clicked() {
                                                        selected_name = name;
                                                        selected_node_idx = node_idx;
                                                        changed = true;
                                                    }
                                                }
                                            });

                                        if changed {
                                            app.composer.track_targets[track_idx] = selected_name;
                                            let _ = app.command_sender.send(Command::Mixer(MixerCommand::SetParam {
                                                target_id: seq_node as u64,
                                                param_id: 10 + track_idx as u32,
                                                value: selected_node_idx as f32,
                                                ramp_duration_samples: 0,
                                            }));
                                        }
                                    });
                                });
                        }

                        if track_idx < 15 {
                            ui.add_space(app.theme.space_xs);
                        }
                    }
                });

                ui.add_space(app.theme.space_sm);

                // 2. RIGHT SIDE: Scrollable Endless Step Grid
                ScrollArea::horizontal()
                    .id_source("composer_endless_grid_scroll_h")
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            // Step Numbers header row
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                                for slot_idx in 0..steps_count {
                                    if slot_idx > 0 && slot_idx % 4 == 0 {
                                        ui.add_space(4.0);
                                    }
                                    let (rect, _) = ui.allocate_exact_size(Vec2::new(24.0, 20.0), Sense::hover());
                                    if slot_idx % 4 == 0 {
                                        let beat_num = (slot_idx / 4) + 1;
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            format!("{}", beat_num),
                                            egui::FontId::new(app.theme.type_caption, egui::FontFamily::Monospace),
                                            app.theme.accent,
                                        );
                                    } else {
                                        let tick_rect = egui::Rect::from_center_size(rect.center(), Vec2::new(2.0, 2.0));
                                        ui.painter().rect_filled(tick_rect, Rounding::same(1.0), app.theme.text_disabled.linear_multiply(0.3));
                                    }
                                }
                            });

                            ui.add_space(app.theme.space_sm);

                            // Render 16 horizontal step rows
                            for track_idx in 0..16 {
                                let track_color = app.theme.track_colors[track_idx];
                                let is_muted = app.composer.track_mutes[track_idx];
                                let is_selected = app.composer.selected_composer_track == Some(track_idx);

                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);

                                    for slot_idx in 0..steps_count {
                                        if slot_idx > 0 && slot_idx % 4 == 0 {
                                            ui.add_space(4.0);
                                        }

                                        let (rect, response) = ui.allocate_exact_size(Vec2::new(24.0, 22.0), Sense::click());

                                        // Dynamic Grid Extension Check: if the last element is visible, mark extend_grid
                                        if slot_idx == steps_count - 1
                                            && ui.is_rect_visible(rect) && steps_count < 512 {
                                                extend_grid = true;
                                            }

                                        // Playback status
                                        let (is_playing, is_starting) = check_step_telemetry(telemetry, track_idx, slot_idx);

                                        let velocity = app.composer.sequencer_grid[grid_deck][track_idx][slot_idx];
                                        let mut color = if is_playing {
                                            app.theme.success
                                        } else if is_starting {
                                            app.theme.warning
                                        } else if velocity > 0.0 {
                                            track_color.gamma_multiply(velocity.clamp(0.5, 1.0))
                                        } else {
                                            track_color.gamma_multiply(0.04)
                                        };

                                        if slot_idx == app.composer.sequencer_active_step {
                                            color = color.linear_multiply(1.4);
                                        }

                                        // Render cell
                                        ui.painter().rect_filled(rect, Rounding::same(2.0), color);
                                        ui.painter().rect_stroke(rect, Rounding::same(2.0), app.theme.border_stroke);

                                        if is_playing {
                                            let tri_p1 = rect.left_center() + Vec2::new(4.0, -4.0);
                                            let tri_p2 = rect.left_center() + Vec2::new(4.0, 4.0);
                                            let tri_p3 = rect.left_center() + Vec2::new(9.0, 0.0);
                                            ui.painter().add(egui::Shape::convex_polygon(vec![tri_p1, tri_p2, tri_p3], app.theme.text_primary, Stroke::NONE));
                                        }

                                        if response.hovered() {
                                            ui.painter().rect_stroke(rect, Rounding::same(2.0), Stroke::new(1.0, app.theme.text_primary));
                                        }

                                        if response.clicked() {
                                            let is_on = app.composer.sequencer_grid[grid_deck][track_idx][slot_idx] == 0.0;
                                            let val = if is_on { 1.0 } else { 0.0 };
                                            app.composer.sequencer_grid[grid_deck][track_idx][slot_idx] = val;
                                            let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetSequencerStep {
                                                node_idx: seq_node,
                                                track: track_idx as u32,
                                                step: slot_idx as u32,
                                                value: val,
                                            }));
                                        }
                                    }
                                });

                                // Accordion expansion in the Right Scrollable side (must match heights and paddings perfectly)
                                if is_selected {
                                    ui.add_space(4.0);
                                    Frame::none()
                                        .fill(app.theme.bg_inset)
                                        .rounding(Rounding::same(app.theme.radius_sm))
                                        .stroke(app.theme.border_stroke)
                                        .inner_margin(Margin::same(4.0))
                                        .show(ui, |ui| {
                                            ui.set_height(50.0);
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new("VOLUME").size(app.theme.type_caption).color(app.theme.text_secondary));
                                                let volume_color = if is_muted { app.theme.bg_inset } else { track_color };
                                                widgets::render_horizontal_fader(ui, &mut app.composer.track_volumes[track_idx], 0.0..=1.0, volume_color, 80.0, 10.0)
                                                    .on_hover_text("VOLUME");

                                                ui.add_space(app.theme.space_md);

                                                ui.label(RichText::new("GENE EVOLVE").size(app.theme.type_caption).color(app.theme.text_secondary));
                                                let mut val = app.composer.evolution_strengths[track_idx];
                                                if widgets::render_horizontal_fader(ui, &mut val, 0.0..=1.0, app.theme.accent, 80.0, 10.0)
                                                    .on_hover_text("GENE EVOLVE")
                                                    .changed()
                                                {
                                                    app.composer.evolution_strengths[track_idx] = val;
                                                    let _ = app.command_sender.send(Command::Performance(PerformanceCommand::EvolvePattern {
                                                        node_idx: track_idx as u32,
                                                        track_idx: 0,
                                                        mutation_strength: val,
                                                    }));

                                                    if let Some(track_id) = app.decks.now_playing[track_idx % 4] {
                                                        use nullherz_dna::GeneticLibrary;
                                                        if let Ok(Some(mut track)) = app.library_db.get_track(track_id) {
                                                            let mut updated_metadata = (*track.metadata).clone();
                                                            for mask_idx in 0..4 {
                                                                let original_mask = updated_metadata.dna.rhythmic.onset_mask[mask_idx];
                                                                let mut mutated_mask = original_mask;
                                                                for bit in 0..64 {
                                                                    let seed = (track_id as u32).wrapping_mul(256).wrapping_add(mask_idx as u32 * 64 + bit as u32);
                                                                    let rand_val = (seed.wrapping_mul(1103515245).wrapping_add(12345) as f32) / 4294967295.0;
                                                                    if rand_val < val {
                                                                        mutated_mask ^= 1 << bit;
                                                                    }
                                                                }
                                                                updated_metadata.dna.rhythmic.onset_mask[mask_idx] = mutated_mask;
                                                            }
                                                            track.metadata = std::sync::Arc::new(updated_metadata);
                                                            let _ = app.library_db.save_track(&track);
                                                            app.library.library_needs_refresh = true;

                                                            if app.breeding_view.parent_a_id.is_none() {
                                                                app.breeding_view.parent_a_id = Some(track_id);
                                                            } else if app.breeding_view.parent_b_id.is_none() || app.breeding_view.parent_b_id == app.breeding_view.parent_a_id {
                                                                app.breeding_view.parent_b_id = Some(track_id);
                                                            } else {
                                                                app.breeding_view.parent_a_id = app.breeding_view.parent_b_id;
                                                                app.breeding_view.parent_b_id = Some(track_id);
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                        });
                                }

                                if track_idx < 15 {
                                    ui.add_space(app.theme.space_xs);
                                }
                            }
                        });
                    });
            });

            if extend_grid {
                for i in 0..16 {
                    app.composer.sequencer_grid[grid_deck][i].resize(steps_count + 16, 0.0);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_core::Telemetry;

    #[test]
    fn test_step_telemetry_check_high_slots() {
        let telemetry = Some(Telemetry::default());

        // Test check_step_telemetry with slot_idx values up to 512
        for track_idx in 0..16 {
            for slot_idx in 0..512 {
                let (is_playing, is_starting) = check_step_telemetry(&telemetry, track_idx, slot_idx);
                assert!(!is_playing, "is_playing must be false for step grid slot {}", slot_idx);
                assert!(!is_starting, "is_starting must be false for step grid slot {}", slot_idx);
            }
        }
    }

    #[test]
    fn test_step_telemetry_none_safety() {
        // Test with None telemetry
        let (is_playing, is_starting) = check_step_telemetry(&None, 0, 100);
        assert!(!is_playing);
        assert!(!is_starting);
    }

    #[test]
    fn test_composer_track_targets_update() {
        let (cmd_tx, _cmd_rx) = std::sync::mpsc::channel();
        let raw_db = nullherz_dna::LibraryDatabase::load(":memory:").expect("Failed to initialize transient LibraryDatabase");
        let db_arc = std::sync::Arc::new(parking_lot::Mutex::new(raw_db));
        let library_db_wrapper = crate::SharedLibraryDb(db_arc);

        let mut app = InspectorApp {
            graph: crate::GraphJson { nodes: vec![], edges: vec![], node_assignments: Default::default() },
            command_sender: cmd_tx,
            last_telemetry: std::sync::Arc::new(parking_lot::Mutex::new(None)),
            active_view: crate::View::Composer,
            mixer: crate::state::MixerState {
                channel_faders: [1.0; 4],
                channel_eq_high: [1.0; 4],
                channel_eq_mid: [1.0; 4],
                channel_eq_low: [1.0; 4],
                channel_filter: [0.5; 4],
                channel_personality_metallic: [0.0; 4],
                channel_personality_organic: [0.0; 4],
                channel_personality_warm: [0.0; 4],
                channel_personality_aggressive: [0.0; 4],
                channel_sync: [false; 4],
                quantize_enabled: true,
                master_gain: 1.0,
                crossfader_pos: 0.5,
                macros: [0.0; 8],
                _macro_names: std::array::from_fn(|i| format!("MACRO {}", i + 1)),
                channel_peak_hold: [0.0; 4],
                master_peak_hold: 0.0,
                _booth_peak_hold: 0.0,
                _rec_peak_hold: 0.0,
                _mastering_eq_enabled: false,
                _mastering_eq_low: 1.0,
                _mastering_eq_mid: 1.0,
                _mastering_eq_high: 1.0,
                spectral_window_shape: 0,
                personality_macro_mode: false,
            },
            decks: crate::state::DeckState {
                master_deck: None,
                now_playing: [None; 4],
                cached_tracks: std::array::from_fn(|_| None),
                global_bpm: 120.0,
                focused_deck: 0,
                deck_playing: [false; 4],
                global_playing: false,
            },
            library: crate::state::LibraryState {
                active_crate: None,
                search_query: String::new(),
                _playlists: vec![],
                cached_library: vec![],
                cached_library_raw: vec![],
                bg_library_loader: None,
                library_needs_refresh: false,
                smart_crate_builder_open: false,
                // smart_crate_def: group default is fine for this test
                selected_library_track: None,
                ingestion_path: String::new(),
                playlist_queue: std::collections::VecDeque::new(),
                ..Default::default()
            },
            composer: crate::state::ComposerState {
                sequencer_grid: std::array::from_fn(|_| std::array::from_fn(|_| vec![0.0; 64])),
                selected_composer_track: None,
                sequencer_active_step: 0,
                track_mutes: [false; 16],
                track_solos: [false; 16],
                track_volumes: [1.0; 16],
                track_targets: std::array::from_fn(|_| "(default)".to_string()),
                record_automation: false,
                _automation_data: std::collections::HashMap::new(),
                evolution_strengths: [0.0; 16],
                auto_pollinate_enabled: false,
            },
            sampler: crate::state::SamplerState {
                sampler_slicer_mode: false,
                sampler_waveform_zoom: 1.0,
                sampler_input_gain: 1.0,
                sampler_monitor_level: 0.0,
                sampler_is_recording: false,
                sampler_is_stereo: false,
                sampler_input_source: 0,
                next_sample_id: 1,
            },
            editor: crate::state::EditorState {
                editor_time_stretch_ratio: 1.0,
                editor_selection: None,
            },
            broadcast: crate::state::BroadcastState {
                is_streaming: false,
                broadcast_url: String::new(),
                broadcast_key: String::new(),
                broadcast_reveal_key: false,
                broadcast_codec: 0,
                broadcast_bitrate: 128.0,
                broadcast_state: 0,
                broadcast_error_msg: String::new(),
                broadcast_start_time: None,
            },
            settings: crate::state::SettingsState {
                active_settings_tab: crate::SettingsTab::General,
                active_backend: nullherz_traits::AudioBackendType::Alsa,
                active_midi_profile: "default".to_string(),
                config_saved_time: None,
                audio_devices: vec![],
                selected_audio_device: String::new(),
                restore_last_session: false,
                default_view_on_launch: crate::View::Composer,
                autosave_enabled: false,
                autosave_interval_mins: 5,
                last_saved_time: 0.0,
                autosave_triggered: None,
                shortcuts_enabled: false,
            },
            viz: crate::state::VizState {
                visualizer_damping: 0.1,
                damped_spectrum: [0.0; 128],
                damped_goniometer: [0.0; 128],
                damped_latent: [0.0; 16],
                damped_peaks: [0.0; 4],
                damped_master_peaks: [0.0; 2],
                last_deck_positions: [0; 4],
            },
            topo: crate::state::TopologyViewState {
                active_connection_source: None,
                active_node_drag: None,
                selected_hotload_node_idx: 0,
                bypassed_nodes: std::collections::HashSet::new(),
                node_map: [("sequencer_node".to_string(), 70), ("sampler_node".to_string(), 100)]
                    .into_iter().collect(),
            },
            library_db: library_db_wrapper,
            active_right_tab: None,
            breeding_view: crate::views::breeder::BreederView::new(),
            wgpu_renderer: None,
            waveform_renderer: None,
            deck_waveform_renderers: [None, None, None, None],
            discovered_sidecars: vec![],
            p2p_sync_success_toast: None,
            export_passport_success_toast: None,
            export_passport_error_toast: None,
            theme: nullherz_ui_hal::Theme::default(),
            last_update_time: 0.0,
            _conductor_thread: None,
        };

        // Assert initial states
        assert_eq!(app.composer.track_targets[0], "(default)");
        assert_eq!(app.composer.track_targets[15], "(default)");

        // Verify node_names retrieval
        let mut names = app.node_names();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(names, vec![("sampler_node".to_string(), 100), ("sequencer_node".to_string(), 70)]);

        // Update a track target
        app.composer.track_targets[3] = "sampler_node".to_string();
        assert_eq!(app.composer.track_targets[3], "sampler_node");
    }
}
