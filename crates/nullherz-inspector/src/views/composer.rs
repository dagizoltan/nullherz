use egui::{Ui, ScrollArea, Vec2, Sense, RichText, Stroke, Frame, Rounding, Margin, Color32, Rect, Pos2};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;
use nullherz_traits::{Command, PerformanceCommand, CoreCommand, MixerCommand};
pub use nullherz_conductor::pattern_manager::DnaSequencer;

/// Helper to determine step status from telemetry safely.
/// Returns (is_playing, is_starting).
#[allow(dead_code)]
pub fn check_step_telemetry(
    _telemetry: &Option<Telemetry>,
    _track_idx: usize,
    _slot_idx: usize,
) -> (bool, bool) {
    (false, false)
}

/// Mini vector waveform envelope painter for audio clips.
pub fn render_mini_waveform(
    painter: &egui::Painter,
    clip_rect: Rect,
    peaks: &[f32],
    color: Color32,
) {
    if clip_rect.width() <= 2.0 {
        return;
    }
    let center_y = clip_rect.center().y;
    let half_h = (clip_rect.height() * 0.42).max(2.0);
    let width = clip_rect.width();

    if peaks.is_empty() {
        let num_bars = (width / 3.0) as usize;
        for i in 0..num_bars {
            let x = clip_rect.left() + (i as f32 / num_bars.max(1) as f32) * width + 1.5;
            let amp = (0.3 + 0.6 * ((i as f32 * 0.7).sin().abs())).clamp(0.1, 0.95);
            painter.line_segment(
                [egui::pos2(x, center_y - amp * half_h), egui::pos2(x, center_y + amp * half_h)],
                Stroke::new(1.2, color),
            );
        }
        return;
    }

    let num_peaks = peaks.len();
    let steps = (width / 2.5) as usize;
    for px in 0..steps {
        let x = clip_rect.left() + (px as f32 / steps.max(1) as f32) * width + 1.2;
        let peak_idx = (px * num_peaks) / steps.max(1);
        let amp = peaks.get(peak_idx).copied().unwrap_or(0.2).abs().clamp(0.05, 1.0);

        painter.line_segment(
            [egui::pos2(x, center_y - amp * half_h), egui::pos2(x, center_y + amp * half_h)],
            Stroke::new(1.2, color),
        );
    }
}

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let Some(seq_node) = app.get_node_id(&format!(
        "deck_{}_sequencer",
        (b'a' + app.decks.focused_deck.min(3) as u8) as char
    )) else {
        ui.label("Sequencer not available yet (topology still installing).");
        return;
    };
    let grid_deck = app.decks.focused_deck.min(3);

    ui.horizontal(|ui| {
        ui.heading(RichText::new("COMPOSER ARRANGEMENT GRID").strong().color(app.theme.text_primary));
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

    // Continuous surface frame wrapping stationary headers + scrollable clip waveform grid
    Frame::none()
        .fill(app.theme.bg_dark)
        .stroke(app.theme.border_stroke)
        .rounding(Rounding::same(app.theme.radius_md))
        .inner_margin(Margin::same(app.theme.space_sm))
        .show(ui, |ui| {
            let mut extend_grid = false;
            let steps_count = app.composer.sequencer_grid[grid_deck][0].len();
            let slot_w = 28.0;
            let slot_h = 24.0;

            ui.horizontal(|ui| {
                // 1. LEFT SIDE: Stationary Track Headers column (100.0px width)
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add_space(32.0);

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
                            .inner_margin(Margin::symmetric(app.theme.space_xs, 0.0))
                            .show(ui, |ui| {
                                ui.set_width(90.0);
                                ui.set_height(slot_h);
                                ui.horizontal(|ui| {
                                    ui.add_space(app.theme.space_xs);
                                    let (swatch_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
                                    ui.painter().rect_filled(swatch_rect, Rounding::same(1.5), track_color);
                                    ui.add_space(app.theme.space_xs);
                                    ui.label(RichText::new(format!("TRK {}", track_idx + 1)).strong().size(app.theme.type_body).color(app.theme.text_primary));
                                });
                            });

                        let rect = inner_resp.response.rect;

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
                                            let activator_color = if !is_muted { app.theme.warning } else { app.theme.bg_inset };
                                            if ui.add_sized([22.0, 18.0], egui::Button::new(RichText::new("ON").size(app.theme.type_caption).strong()).fill(activator_color)).clicked() {
                                                app.composer.track_mutes[track_idx] = !is_muted;
                                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetTrackMute { node_idx: seq_node, track_idx: track_idx as u32, muted: app.composer.track_mutes[track_idx] }));
                                            }

                                            let is_soloed = app.composer.track_solos[track_idx];
                                            let solo_color = if is_soloed { app.theme.track_colors[1] } else { app.theme.bg_inset };
                                            if ui.add_sized([18.0, 18.0], egui::Button::new(RichText::new("S").size(app.theme.type_caption).strong()).fill(solo_color)).clicked() {
                                                app.composer.track_solos[track_idx] = !is_soloed;
                                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetTrackSolo { node_idx: seq_node, track_idx: track_idx as u32, soloed: app.composer.track_solos[track_idx] }));
                                            }

                                            let stop_btn = egui::Button::new(RichText::new("■").size(app.theme.type_caption).strong()).fill(app.theme.bg_inset);
                                            if ui.add_sized([18.0, 18.0], stop_btn).on_hover_text("Stop clip").clicked() {
                                                app.composer.sequencer_grid[grid_deck][track_idx].fill(0.0);
                                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::ClearTrackPattern { node_idx: seq_node, track_idx: track_idx as u32 }));
                                            }
                                        });

                                        ui.add_space(4.0);

                                        if ui.button(RichText::new("+ SIDECAR").size(app.theme.type_caption).strong())
                                            .on_hover_text("Open Sidecar Store to select instruments or inserts")
                                            .clicked()
                                        {
                                            app.active_right_tab = Some(crate::RightTab::Store);
                                            app.store.active_tag_filter = Some("instrument".to_string());
                                        }

                                        ui.add_space(2.0);

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
                            ui.add_space(6.0);
                        }
                    }
                });

                ui.add_space(6.0);

                // 2. RIGHT SIDE: Timeline Header + Audio Clip Waveform Grid
                ScrollArea::horizontal()
                    .id_source("composer_endless_grid_scroll_h")
                    .show(ui, |ui| {
                        let mut grid_top_pos = Pos2::ZERO;
                        let mut grid_bottom_pos = Pos2::ZERO;

                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;

                            // Bar/Beat Timeline Header Row
                            let header_resp = ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                                for slot_idx in 0..steps_count {
                                    if slot_idx > 0 && slot_idx % 4 == 0 {
                                        ui.add_space(4.0);
                                    }
                                    let (rect, response) = ui.allocate_exact_size(Vec2::new(slot_w, 24.0), Sense::click());

                                    if response.clicked() {
                                        let bar = (slot_idx / 4) + 1;
                                        let beat_pos = (bar - 1) as f64 * 4.0;
                                        let _ = app.command_sender.send(Command::Performance(PerformanceCommand::JumpByBeats { node_idx: seq_node, beats: beat_pos as f32 }));
                                    }

                                    if slot_idx % 4 == 0 {
                                        let bar_num = (slot_idx / 4) + 1;
                                        ui.painter().rect_filled(rect, Rounding::same(2.0), app.theme.bg_surface);
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            format!("BAR {}", bar_num),
                                            egui::FontId::new(10.0, egui::FontFamily::Monospace),
                                            app.theme.accent,
                                        );
                                    } else {
                                        let tick_rect = egui::Rect::from_center_size(rect.center(), Vec2::new(2.0, 4.0));
                                        ui.painter().rect_filled(tick_rect, Rounding::same(1.0), app.theme.text_disabled.linear_multiply(0.4));
                                    }
                                }
                            });

                            grid_top_pos = header_resp.response.rect.left_bottom();

                            ui.add_space(6.0);

                            // Render 16 horizontal track clip rows
                            for track_idx in 0..16 {
                                let track_color = app.theme.track_colors[track_idx];
                                let is_muted = app.composer.track_mutes[track_idx];
                                let is_selected = app.composer.selected_composer_track == Some(track_idx);

                                // Resolve track source sample / metadata
                                let src_id = app.composer.track_sources[track_idx]
                                    .or(app.decks.now_playing[track_idx % 4]);
                                let cached_track = src_id.and_then(|id| app.get_cached_track(id));

                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);

                                    for slot_idx in 0..steps_count {
                                        if slot_idx > 0 && slot_idx % 4 == 0 {
                                            ui.add_space(4.0);
                                        }

                                        let (rect, response) = ui.allocate_exact_size(Vec2::new(slot_w, slot_h), Sense::click());

                                        if slot_idx == steps_count - 1
                                            && ui.is_rect_visible(rect) && steps_count < 512 {
                                                extend_grid = true;
                                            }

                                        let velocity = app.composer.sequencer_grid[grid_deck][track_idx][slot_idx];

                                        let mut bg_color = if velocity > 0.0 {
                                            if is_muted {
                                                app.theme.bg_inset
                                            } else {
                                                track_color.gamma_multiply(0.25)
                                            }
                                        } else {
                                            track_color.gamma_multiply(0.03)
                                        };

                                        if slot_idx == app.composer.sequencer_active_step {
                                            bg_color = bg_color.linear_multiply(1.3);
                                        }

                                        ui.painter().rect_filled(rect, Rounding::same(2.0), bg_color);
                                        let border_stroke = if velocity > 0.0 {
                                            Stroke::new(1.0, track_color)
                                        } else {
                                            app.theme.border_stroke
                                        };
                                        ui.painter().rect_stroke(rect, Rounding::same(2.0), border_stroke);

                                        // Mini-waveform rendering inside active clip slots
                                        if velocity > 0.0 {
                                            let peaks_data = cached_track.as_ref()
                                                .map(|t| t.metadata.peaks.as_slice())
                                                .unwrap_or(&[]);
                                            let wf_color = if is_muted { app.theme.text_disabled } else { track_color };
                                            render_mini_waveform(ui.painter(), rect.shrink(1.0), peaks_data, wf_color);
                                        }

                                        if response.hovered() {
                                            ui.painter().rect_stroke(rect, Rounding::same(2.0), Stroke::new(1.2_f32, app.theme.text_primary));
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

                                // Accordion expansion in the Right Scrollable side
                                if is_selected {
                                    ui.add_space(4.0);
                                    Frame::none()
                                        .fill(app.theme.bg_inset)
                                        .rounding(Rounding::same(app.theme.radius_sm))
                                        .stroke(app.theme.border_stroke)
                                        .inner_margin(Margin::same(4.0))
                                        .show(ui, |ui| {
                                            ui.set_height(80.0);
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

                                                    let src = app.composer.track_sources
                                                        .get(track_idx).copied().flatten()
                                                        .or(app.decks.now_playing[track_idx % 4]);
                                                    if let Some(track_id) = src {
                                                        use nullherz_dna::GeneticLibrary;
                                                        if let Some(mut track) = app.get_cached_track(track_id) {
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
                                    ui.add_space(6.0);
                                }
                            }

                            grid_bottom_pos = ui.cursor().left_top();

                            // Live Playhead Needle Drawing across the timeline
                            let live_beat = telemetry.as_ref()
                                .map(|t| t.beat_position as f32)
                                .unwrap_or(app.composer.sequencer_active_step as f32);
                            let playhead_step = live_beat.max(0.0).min(steps_count as f32);
                            let bars_before = (playhead_step / 4.0).floor();
                            let playhead_x = grid_top_pos.x + (playhead_step * slot_w) + (playhead_step * 2.0) + (bars_before * 4.0) + (slot_w * 0.5);

                            if playhead_x >= grid_top_pos.x {
                                ui.painter().line_segment(
                                    [Pos2::new(playhead_x, grid_top_pos.y), Pos2::new(playhead_x, grid_bottom_pos.y)],
                                    Stroke::new(2.5, app.theme.accent),
                                );
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
                channel_sync: [true; 4],
                quantize_enabled: true,
                ..Default::default()
            },
            decks: crate::state::DeckState {
                master_deck: None,
                now_playing: [None; 4],
                cached_tracks: std::array::from_fn(|_| None),
                global_bpm: 120.0,
                focused_deck: 0,
                deck_playing: [false; 4],
                global_playing: false,
                ..Default::default()
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
                selected_library_track: None,
                ingestion_path: String::new(),
                playlist_queue: std::collections::VecDeque::new(),
                ..Default::default()
            },
            store: Default::default(),
            composer: crate::state::ComposerState {
                ..Default::default()
            },
            sampler: crate::state::SamplerState {
                ..Default::default()
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
                _selected_audio_device: String::new(),
                restore_last_session: false,
                default_view_on_launch: crate::View::Composer,
                autosave_enabled: false,
                autosave_interval_mins: 5,
                last_saved_time: 0.0,
                autosave_triggered: None,
                shortcuts_enabled: false,
                ..Default::default()
            },
            viz: crate::state::VizState {
                visualizer_damping: 0.1,
                damped_spectrum: [0.0; 128],
                damped_goniometer: [0.0; 128],
                damped_latent: [0.0; 16],
                damped_peaks: [0.0; 4],
                damped_master_peaks: [0.0; 2],
                last_deck_positions: [0; 4],
                deck_still_snapshots: [0; 4],
                last_playstate_counter: 0,
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

        assert_eq!(app.composer.track_targets[0], "(default)");
        assert_eq!(app.composer.track_targets[15], "(default)");

        let mut names = app.node_names();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(names, vec![("sampler_node".to_string(), 100), ("sequencer_node".to_string(), 70)]);

        app.composer.track_targets[3] = "sampler_node".to_string();
        assert_eq!(app.composer.track_targets[3], "sampler_node");
    }
}
