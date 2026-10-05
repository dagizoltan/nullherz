use egui::{Ui, ScrollArea, Vec2, Sense, RichText, Stroke, Frame, Rounding, Margin, Color32, Rect, Pos2};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;
use nullherz_traits::{Command, PerformanceCommand, CoreCommand};
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

/// Mini vector waveform envelope painter for audio clips and beatgrid-aligned tracks.
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
                Stroke::new(1.2_f32, color),
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
            Stroke::new(1.2_f32, color),
        );
    }
}

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let focused_deck_char = (b'a' + (app.decks.focused_deck % 4) as u8) as char;
    let seq_node = app.get_node_id(&format!("deck_{}_sequencer", focused_deck_char)).unwrap_or(70);

    ui.horizontal(|ui| {
        ui.heading(RichText::new("COMPOSER ARRANGEMENT GRID").strong().color(app.theme.text_primary));
        ui.add_space(app.theme.space_md);
        ui.label(RichText::new(format!("STUDIO DAW ENGINE: {} TRACKS", app.mixer.num_channels)).strong().size(app.theme.type_caption).color(app.theme.accent));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
             ui.label(egui::RichText::new("DECOUPLED STUDIO DAW").color(app.theme.accent).size(app.theme.type_caption));
        });
    });
    ui.add_space(app.theme.space_sm);

    // Studio Transport & Master Controls
    ui.horizontal(|ui| {
        // Independent Composer Transport PLAY / STOP Controls
        let play_btn = ui.selectable_label(app.composer.composer_playing, RichText::new("▶ PLAY COMPOSER").strong().color(if app.composer.composer_playing { app.theme.success } else { app.theme.text_secondary }));
        if play_btn.clicked() {
            app.composer.composer_playing = true;
            let _ = app.command_sender.send(Command::Core(CoreCommand::Play));
        }

        let stop_btn = ui.selectable_label(!app.composer.composer_playing, RichText::new("■ STOP COMPOSER").strong().color(if !app.composer.composer_playing { app.theme.danger } else { app.theme.text_secondary }));
        if stop_btn.clicked() {
            app.composer.composer_playing = false;
            let _ = app.command_sender.send(Command::Core(CoreCommand::Stop));
        }

        ui.add_space(app.theme.space_sm);

        // Master Transport Sync Toggle
        let is_synced = app.composer.sync_with_master_transport;
        ui.toggle_value(&mut app.composer.sync_with_master_transport, RichText::new("🔒 SYNC MASTER CLOCK").color(if is_synced { app.theme.accent } else { app.theme.text_secondary }));

        ui.add_space(app.theme.space_md);

        // Global BPM
        ui.label(RichText::new("BPM").strong().size(app.theme.type_caption).color(app.theme.text_secondary));
        let mut bpm = app.decks.global_bpm;
        if ui.add(egui::DragValue::new(&mut bpm).speed(0.1).range(20.0..=300.0)).changed() {
            app.decks.global_bpm = bpm;
            let _ = app.command_sender.send(Command::Core(CoreCommand::SetBpm(bpm)));
        }

        ui.add_space(app.theme.space_md);

        // Beatgrid Precision Selector (1/16, 1/32, 1/64)
        ui.label(RichText::new("PRECISION:").strong().size(app.theme.type_caption).color(app.theme.text_secondary));
        let steps_per_bar = app.composer.grid_step_resolution.clamp(16, 64);
        egui::ComboBox::from_id_source("grid_precision_select")
            .width(65.0)
            .selected_text(RichText::new(format!("1/{}", steps_per_bar)).size(app.theme.type_caption).strong())
            .show_ui(ui, |ui| {
                for res in [16, 32, 64] {
                    if ui.selectable_label(app.composer.grid_step_resolution == res, format!("1/{} Note", res)).clicked() {
                        app.composer.grid_step_resolution = res;
                    }
                }
            });

        ui.add_space(app.theme.space_md);

        // Pattern Length Selector
        ui.label(RichText::new("LENGTH:").strong().size(app.theme.type_caption).color(app.theme.text_secondary));
        let current_steps = app.composer.studio_sequencer_grid[0].len();
        let current_bars = (current_steps / steps_per_bar).max(1);
        let mut selected_bars = current_bars;

        egui::ComboBox::from_id_source("pattern_length_select")
            .width(70.0)
            .selected_text(RichText::new(format!("{} BARS", current_bars)).size(app.theme.type_caption).strong())
            .show_ui(ui, |ui| {
                for bars in [1, 2, 4, 8] {
                    if ui.selectable_label(current_bars == bars, format!("{} Bars ({} steps)", bars, bars * steps_per_bar)).clicked() {
                        selected_bars = bars;
                    }
                }
            });

        let target_steps = selected_bars * steps_per_bar;
        if target_steps != current_steps {
            for trk in 0..16 {
                app.composer.studio_sequencer_grid[trk].resize(target_steps, 0.0);
            }
        }

        ui.add_space(app.theme.space_md);

        // Interactive Timeline Zoom Control
        ui.label(RichText::new("🔍 ZOOM:").size(app.theme.type_caption).strong().color(app.theme.text_secondary));
        ui.add(egui::Slider::new(&mut app.composer.grid_zoom, 0.5..=3.0).show_value(false));

        ui.add_space(app.theme.space_md);

        let is_recording = app.composer.record_automation;
        ui.toggle_value(&mut app.composer.record_automation, RichText::new("🔴 RECORD AUTOMATION").color(if is_recording { app.theme.danger } else { app.theme.text_secondary }));
        ui.add_space(app.theme.space_md);

        let is_kbd_open = app.composer.keyboard_grid.is_open;
        ui.toggle_value(&mut app.composer.keyboard_grid.is_open, RichText::new("🎹 KEYBOARD GRID").color(if is_kbd_open { app.theme.accent } else { app.theme.text_secondary }));
        ui.add_space(app.theme.space_md);

        if ui.button("STOP ALL CLIPS").clicked() {
            for i in 0..16 {
                 let _ = app.command_sender.send(Command::Performance(PerformanceCommand::ClearTrackPattern { node_idx: seq_node, track_idx: i as u32 }));
                 app.composer.studio_sequencer_grid[i].fill(0.0);
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
            let steps_count = app.composer.studio_sequencer_grid[0].len();
            let slot_w = (40.0 * app.composer.grid_zoom).clamp(15.0, 120.0);
            let slot_h = 88.0;
            let num_active_channels = app.mixer.num_channels.clamp(1, 16);

            ui.horizontal(|ui| {
                // 1. LEFT SIDE: Decoupled Studio Track Headers column (175.0px width)
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add_space(32.0); // Exact match: 26.0 timeline header + 6.0 gap

                    for track_idx in 0..num_active_channels {
                        let track_color = crate::InspectorApp::deck_color(&app.theme, track_idx % 4);
                        let is_muted = app.composer.track_mutes[track_idx];
                        let is_solo = app.composer.track_solos[track_idx];
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
                            .stroke(Stroke::new(1.0_f32, if is_selected { track_color } else { app.theme.border_stroke.color }))
                            .inner_margin(Margin::same(4.0))
                            .show(ui, |ui| {
                                ui.set_width(165.0);
                                ui.set_height(slot_h - 8.0);
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        let (swatch_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
                                        ui.painter().rect_filled(swatch_rect, Rounding::same(1.5), track_color);
                                        ui.add_space(2.0);
                                        ui.label(RichText::new(format!("TRK {:02}", track_idx + 1)).strong().size(app.theme.type_body).color(app.theme.text_primary));

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            // Mute Button
                                            let mute_color = if is_muted { app.theme.danger } else { app.theme.bg_inset };
                                            if ui.add_sized([18.0, 18.0], egui::Button::new(RichText::new("M").size(app.theme.type_caption).strong()).fill(mute_color)).clicked() {
                                                app.composer.track_mutes[track_idx] = !is_muted;
                                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetTrackMute { node_idx: seq_node, track_idx: track_idx as u32, muted: app.composer.track_mutes[track_idx] }));
                                            }

                                            // Solo Button
                                            let solo_color = if is_solo { app.theme.warning } else { app.theme.bg_inset };
                                            if ui.add_sized([18.0, 18.0], egui::Button::new(RichText::new("S").size(app.theme.type_caption).strong()).fill(solo_color)).clicked() {
                                                app.composer.track_solos[track_idx] = !is_solo;
                                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetTrackSolo { node_idx: seq_node, track_idx: track_idx as u32, soloed: app.composer.track_solos[track_idx] }));
                                            }

                                            // + SIDECAR FX Store Button
                                            if ui.add_sized([38.0, 18.0], egui::Button::new(RichText::new("+ SIDECAR").size(6.5).strong()).fill(app.theme.accent.linear_multiply(0.2))).on_hover_text("Open Sidecar Store for Studio Inserts & Instruments").clicked() {
                                                app.store.search_query = "insert".to_string();
                                                app.active_right_tab = Some(crate::RightTab::Store);
                                            }
                                        });
                                    });

                                    ui.add_space(2.0);

                                    // Sample / Full-Track Picker dropdown
                                    let src_id = app.composer.track_sources[track_idx];
                                    let cached_track = src_id.and_then(|id| app.get_cached_track(id));
                                    let sample_label = cached_track.as_ref()
                                        .map(|t| format!("♪ {}", t.title))
                                        .unwrap_or_else(|| "⊕ SAMPLE / TRACK".to_string());

                                    egui::ComboBox::from_id_source(format!("seq_studio_src_{}", track_idx))
                                        .width(155.0)
                                        .selected_text(RichText::new(&sample_label).size(app.theme.type_caption).strong())
                                        .show_ui(ui, |ui| {
                                            if ui.selectable_label(src_id.is_none(), "(None)").clicked() {
                                                app.composer.track_sources[track_idx] = None;
                                            }
                                            for lib_track in &app.library.cached_library_raw {
                                                let is_sel = src_id == Some(lib_track.id);
                                                let label = format!("♪ {}", lib_track.title);
                                                if ui.selectable_label(is_sel, label).clicked() {
                                                    app.composer.track_sources[track_idx] = Some(lib_track.id);
                                                }
                                            }
                                        });

                                    ui.add_space(2.0);

                                    // Volume Fader & Pan Slider
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("VOL").size(8.0).color(app.theme.text_secondary));
                                        let mut vol_val = app.composer.track_volumes[track_idx];
                                        if widgets::render_horizontal_fader(ui, &mut vol_val, 0.0..=1.2, track_color, 65.0, 10.0).changed() {
                                            app.composer.track_volumes[track_idx] = vol_val;
                                            app.mixer.channel_faders[track_idx] = vol_val;
                                        }

                                        ui.add_space(2.0);
                                        ui.label(RichText::new("PAN").size(8.0).color(app.theme.text_secondary));
                                        let mut pan_val = app.composer.track_pans[track_idx];
                                        if ui.add(egui::Slider::new(&mut pan_val, -1.0..=1.0).show_value(false)).changed() {
                                            app.composer.track_pans[track_idx] = pan_val;
                                        }
                                    });
                                });
                            });

                        let rect = inner_resp.response.rect;
                        let response = ui.interact(rect, ui.make_persistent_id(format!("trk_hdr_studio_{}", track_idx)), Sense::click());
                        if response.clicked() {
                            app.composer.selected_composer_track = Some(track_idx);
                        }

                        if track_idx < num_active_channels - 1 {
                            ui.add_space(6.0);
                        }
                    }
                });

                ui.add_space(6.0);

                // 2. RIGHT SIDE: Timeline Header + Audio Clip / Full Track Waveform Grid
                ScrollArea::horizontal()
                    .id_source("composer_endless_grid_scroll_h")
                    .show(ui, |ui| {
                        let mut grid_top_pos = Pos2::ZERO;
                        let mut grid_bottom_pos = Pos2::ZERO;

                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;

                            // Bar/Beat Timeline Header Row (26.0px height)
                            let steps_per_bar = app.composer.grid_step_resolution.clamp(16, 64);
                            let steps_per_beat = (steps_per_bar / 4).max(1);

                            let header_resp = ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                                for slot_idx in 0..steps_count {
                                    if slot_idx > 0 && slot_idx % steps_per_beat == 0 {
                                        ui.add_space(4.0);
                                    }
                                    let (rect, response) = ui.allocate_exact_size(Vec2::new(slot_w, 24.0), Sense::click());

                                    if response.clicked() {
                                        let bar = (slot_idx / steps_per_bar) + 1;
                                        let beat_pos = (bar - 1) as f64 * 4.0;
                                        let _ = app.command_sender.send(Command::Performance(PerformanceCommand::JumpByBeats { node_idx: seq_node, beats: beat_pos as f32 }));
                                    }

                                    if slot_idx % steps_per_bar == 0 {
                                        let bar_num = (slot_idx / steps_per_bar) + 1;
                                        ui.painter().rect_filled(rect, Rounding::same(2.0), app.theme.bg_surface);
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            format!("BAR {}", bar_num),
                                            egui::FontId::new(10.0, egui::FontFamily::Monospace),
                                            app.theme.accent,
                                        );
                                    } else if slot_idx % steps_per_beat == 0 {
                                        let beat_num = (slot_idx % steps_per_bar) / steps_per_beat + 1;
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            format!(".{}", beat_num),
                                            egui::FontId::new(9.0, egui::FontFamily::Monospace),
                                            app.theme.text_secondary,
                                        );
                                    } else {
                                        let tick_rect = egui::Rect::from_center_size(rect.center(), Vec2::new(2.0, 4.0));
                                        ui.painter().rect_filled(tick_rect, Rounding::same(1.0), app.theme.text_disabled.linear_multiply(0.4));
                                    }
                                }
                            });

                            grid_top_pos = header_resp.response.rect.left_bottom();

                            ui.add_space(6.0);

                            // Render active channel rows in step grid
                            for track_idx in 0..num_active_channels {
                                let track_color = crate::InspectorApp::deck_color(&app.theme, track_idx % 4);
                                let is_muted = app.composer.track_mutes[track_idx];
                                let is_selected = app.composer.selected_composer_track == Some(track_idx);

                                // Resolve track source sample / metadata
                                let src_id = app.composer.track_sources[track_idx];
                                let cached_track = src_id.and_then(|id| app.get_cached_track(id));

                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);

                                    let steps_per_bar = app.composer.grid_step_resolution.clamp(16, 64);
                                    let steps_per_beat = (steps_per_bar / 4).max(1);

                                    for slot_idx in 0..steps_count {
                                        if slot_idx > 0 && slot_idx % steps_per_beat == 0 {
                                            ui.add_space(4.0);
                                        }
                                        if slot_idx > 0 && slot_idx % steps_per_bar == 0 {
                                            ui.add_space(6.0);
                                        }

                                        let (rect, response) = ui.allocate_exact_size(Vec2::new(slot_w, slot_h), Sense::click());

                                        if slot_idx == steps_count - 1
                                            && ui.is_rect_visible(rect) && steps_count < 512 {
                                                extend_grid = true;
                                            }

                                        let velocity = app.composer.studio_sequencer_grid[track_idx][slot_idx];

                                        let mut bg_color = if velocity > 0.0 || cached_track.is_some() {
                                            if is_muted {
                                                app.theme.bg_inset
                                            } else {
                                                track_color.gamma_multiply(0.25)
                                            }
                                        } else {
                                            track_color.gamma_multiply(0.03)
                                        };

                                        if slot_idx == app.composer.sequencer_active_step {
                                            bg_color = track_color.gamma_multiply(0.5);
                                        }

                                        ui.painter().rect_filled(rect, Rounding::same(2.0), bg_color);
                                        let border_stroke = if velocity > 0.0 {
                                            Stroke::new(1.0_f32, track_color)
                                        } else {
                                            app.theme.border_stroke
                                        };
                                        ui.painter().rect_stroke(rect, Rounding::same(2.0), border_stroke);

                                        // Render mini audio waveforms or beatgrid-aligned full track slice
                                        if let Some(ref t) = cached_track {
                                            let peaks = t.metadata.peaks.as_slice();
                                            if !peaks.is_empty() {
                                                let total_peaks = peaks.len();
                                                let slice_len = (total_peaks / steps_count.max(1)).max(1);
                                                let start_idx = (slot_idx * slice_len).min(total_peaks.saturating_sub(1));
                                                let end_idx = ((slot_idx + 1) * slice_len).min(total_peaks);
                                                let slice_peaks = &peaks[start_idx..end_idx];

                                                let wf_color = if is_muted { app.theme.text_disabled } else { track_color };
                                                render_mini_waveform(ui.painter(), rect.shrink(2.0), slice_peaks, wf_color);
                                            }
                                        } else if velocity > 0.0 {
                                            let wf_color = if is_muted { app.theme.text_disabled } else { track_color };
                                            render_mini_waveform(ui.painter(), rect.shrink(2.0), &[], wf_color);
                                        }

                                        if response.hovered() {
                                            ui.painter().rect_stroke(rect, Rounding::same(2.0), Stroke::new(1.2_f32, app.theme.text_primary));
                                        }

                                        if response.clicked() {
                                            let is_on = app.composer.studio_sequencer_grid[track_idx][slot_idx] == 0.0;
                                            let val = if is_on { 1.0 } else { 0.0 };
                                            app.composer.studio_sequencer_grid[track_idx][slot_idx] = val;
                                            let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetSequencerStep {
                                                node_idx: seq_node,
                                                track: track_idx as u32,
                                                step: slot_idx as u32,
                                                value: val,
                                            }));
                                        }
                                    }
                                });

                                // 16-Subchannel Subgrid Composition for Selected/Instrument Track
                                if is_selected {
                                    ui.add_space(4.0);
                                    let sub_labels = [
                                        "SUB 01 KICK", "SUB 02 SNARE", "SUB 03 HH-CL", "SUB 04 HH-OP",
                                        "SUB 05 TOM-LO", "SUB 06 TOM-MID", "SUB 07 TOM-HI", "SUB 08 PERC 1",
                                        "SUB 09 PERC 2", "SUB 10 CLAP", "SUB 11 RIDE", "SUB 12 CRASH",
                                        "SUB 13 FX 1", "SUB 14 FX 2", "SUB 15 AUX 1", "SUB 16 AUX 2",
                                    ];

                                    for sub_i in 0..16 {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                                            let sub_label = sub_labels[sub_i];

                                            for slot_idx in 0..steps_count {
                                                if slot_idx > 0 && slot_idx % steps_per_beat == 0 {
                                                    ui.add_space(4.0);
                                                }
                                                if slot_idx > 0 && slot_idx % steps_per_bar == 0 {
                                                    ui.add_space(6.0);
                                                }

                                                let (rect, response) = ui.allocate_exact_size(Vec2::new(slot_w, 22.0), Sense::click());
                                                let vel_sub = app.composer.studio_sequencer_grid[sub_i % 16][slot_idx];

                                                let bg_sub = if vel_sub > 0.0 {
                                                    track_color.gamma_multiply(0.4)
                                                } else {
                                                    track_color.gamma_multiply(0.05)
                                                };

                                                ui.painter().rect_filled(rect, Rounding::same(2.0), bg_sub);
                                                ui.painter().rect_stroke(rect, Rounding::same(2.0), Stroke::new(0.8_f32, if vel_sub > 0.0 { track_color } else { app.theme.border_stroke.color }));

                                                if slot_idx == 0 {
                                                    ui.painter().text(
                                                        rect.left_center() + Vec2::new(4.0, 0.0),
                                                        egui::Align2::LEFT_CENTER,
                                                        sub_label,
                                                        egui::FontId::new(7.5, egui::FontFamily::Monospace),
                                                        app.theme.text_primary,
                                                    );
                                                }

                                                if response.clicked() {
                                                    let is_on = app.composer.studio_sequencer_grid[sub_i % 16][slot_idx] == 0.0;
                                                    let val = if is_on { 1.0 } else { 0.0 };
                                                    app.composer.studio_sequencer_grid[sub_i % 16][slot_idx] = val;
                                                    if let Some(target_node) = app.get_node_id("drum_machine_node").or(Some(seq_node)) {
                                                        let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetSequencerStep {
                                                            node_idx: target_node,
                                                            track: sub_i as u32,
                                                            step: slot_idx as u32,
                                                            value: val,
                                                        }));
                                                    }
                                                }
                                            }
                                        });
                                        ui.add_space(2.0);
                                    }
                                }

                                if track_idx < num_active_channels - 1 {
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
                                    Stroke::new(2.5_f32, app.theme.accent),
                                );
                            }
                        });
                    });
            });

            if extend_grid {
                for i in 0..16 {
                    app.composer.studio_sequencer_grid[i].resize(steps_count + 16, 0.0);
                }
            }
        });

    if app.composer.keyboard_grid.is_open {
        ui.add_space(app.theme.space_sm);
        Frame::none()
            .fill(app.theme.bg_surface)
            .rounding(Rounding::same(app.theme.radius_md))
            .stroke(app.theme.border_stroke)
            .inner_margin(Margin::same(app.theme.space_sm))
            .show(ui, |ui| {
                let selected_trk = app.composer.selected_composer_track.unwrap_or(0);
                let deck_color = crate::InspectorApp::deck_color(&app.theme, selected_trk % 4);
                let note_triggers = widgets::render_keyboard_grid(
                    ui,
                    &mut app.composer.keyboard_grid.octave,
                    &mut app.composer.keyboard_grid.style,
                    &app.composer.keyboard_grid.active_held_notes,
                    deck_color,
                );

                for trig in note_triggers {
                    let event = nullherz_traits::MidiEvent {
                        timestamp_samples: 0,
                        status: if trig.is_note_on { 0x90 } else { 0x80 },
                        data1: trig.note,
                        data2: trig.velocity,
                        _pad: 0,
                    };
                    let _ = app.command_sender.send(Command::Core(CoreCommand::InjectMidi(event)));
                }
            });
    }
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
    fn test_composer_grid_zoom_and_row_height_sync() {
        let mut state = crate::state::ComposerState::default();
        assert_eq!(state.grid_zoom, 1.0);
        state.grid_zoom = 2.0;

        let slot_w = (40.0 * state.grid_zoom).clamp(15.0, 120.0);
        assert_eq!(slot_w, 80.0, "Zoom 2.0 should result in 80px slot width");
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
            detached_views: std::collections::HashSet::new(),
            mixer: crate::state::MixerState {
                channel_sync: [true; 16],
                quantize_enabled: true,
                ..Default::default()
            },
            decks: crate::state::DeckState {
                master_deck: None,
                now_playing: [None; 16],
                cached_tracks: std::array::from_fn(|_| None),
                global_bpm: 120.0,
                focused_deck: 0,
                deck_playing: [false; 16],
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
            viz: crate::state::VizState::default(),
            topo: crate::state::TopologyViewState {
                active_connection_source: None,
                active_node_drag: None,
                selected_hotload_node_idx: 0,
                bypassed_nodes: std::collections::HashSet::new(),
                node_map: [("sequencer_node".to_string(), 70), ("sampler_node".to_string(), 100)]
                    .into_iter().collect(),
            },
            analyzer: Default::default(),
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
            rt_warnings: vec![],
            theme: nullherz_ui_hal::Theme::default(),
            last_update_time: 0.0,
            last_telemetry_time: 0.0,
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
