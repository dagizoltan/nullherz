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
    let half_h = (clip_rect.height() * 0.45).max(2.0);
    let width = clip_rect.width();

    if peaks.is_empty() {
        let num_bars = (width / 2.5) as usize;
        for i in 0..num_bars {
            let x = clip_rect.left() + (i as f32 / num_bars.max(1) as f32) * width + 1.2;
            let amp = (0.3 + 0.6 * ((i as f32 * 0.7).sin().abs())).clamp(0.1, 0.95);
            painter.line_segment(
                [egui::pos2(x, center_y - amp * half_h), egui::pos2(x, center_y + amp * half_h)],
                Stroke::new(1.0_f32, color.linear_multiply(0.4)),
            );
        }
        return;
    }

    let num_peaks = peaks.len();
    let steps = (width * 1.5).max(4.0) as usize;
    for px in 0..steps {
        let x = clip_rect.left() + (px as f32 / steps as f32) * width;
        let p_start = (px * num_peaks) / steps;
        let p_end = (((px + 1) * num_peaks) / steps).max(p_start + 1);

        let window_slice = &peaks[p_start.min(num_peaks.saturating_sub(1))..p_end.min(num_peaks)];
        let max_amp = window_slice.iter().fold(0.0f32, |a, &v| a.max(v.abs())).clamp(0.02, 1.0);

        painter.line_segment(
            [egui::pos2(x, center_y - max_amp * half_h), egui::pos2(x, center_y + max_amp * half_h)],
            Stroke::new(1.0_f32, color),
        );
    }
}

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let seq_node = app.get_node_id("deck_a_sequencer").unwrap_or(70);

    ui.horizontal(|ui| {
        ui.heading(RichText::new("COMPOSER ARRANGEMENT GRID").strong().color(app.theme.text_primary));
        ui.add_space(app.theme.space_md);
        ui.label(RichText::new(format!("STUDIO DAW ENGINE: {} TRACKS", app.mixer.num_channels)).strong().size(app.theme.type_caption).color(app.theme.accent));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new("DECOUPLED STUDIO DAW").color(app.theme.accent).size(app.theme.type_caption));
        });
    });
    ui.add_space(app.theme.space_sm);

    // Modern Studio Transport Bar
    ui.horizontal(|ui| {
        let is_recording = app.composer.record_automation;
        if ui.add(egui::Button::new(RichText::new(format!("{} REC", egui_phosphor::regular::RECORD)).size(9.0).strong()).fill(if is_recording { app.theme.danger } else { app.theme.bg_inset })).on_hover_text("Record Automation").clicked() {
            app.composer.record_automation = !app.composer.record_automation;
        }

        ui.add_space(app.theme.space_xs);

        let play_icon = if app.composer.composer_playing { egui_phosphor::regular::PAUSE } else { egui_phosphor::regular::PLAY };
        let play_bg = if app.composer.composer_playing { app.theme.success } else { app.theme.bg_inset };

        if ui.add_sized([28.0, 24.0], egui::Button::new(RichText::new(play_icon).size(12.0).strong()).fill(play_bg)).on_hover_text("Play / Pause Transport").clicked() {
            app.composer.composer_playing = !app.composer.composer_playing;
            if app.composer.composer_playing {
                let _ = app.command_sender.send(Command::Core(CoreCommand::Play));
            } else {
                let _ = app.command_sender.send(Command::Core(CoreCommand::Stop));
            }
        }

        if ui.add_sized([28.0, 24.0], egui::Button::new(RichText::new(egui_phosphor::regular::SQUARE).size(12.0).strong()).fill(app.theme.bg_inset)).on_hover_text("Stop Transport").clicked() {
            app.composer.composer_playing = false;
            let _ = app.command_sender.send(Command::Core(CoreCommand::Stop));
        }

        ui.add_space(app.theme.space_xs);

        // Status indicator pill
        let status_text = if app.composer.composer_playing { "COMPOSER RUNNING" } else { "STOPPED" };
        let status_col = if app.composer.composer_playing { app.theme.success } else { app.theme.text_secondary };
        ui.label(RichText::new(status_text).strong().size(9.0).color(status_col));

        ui.add_space(app.theme.space_md);

        let is_synced = app.composer.sync_with_master_transport;
        let sync_icon = if is_synced { egui_phosphor::regular::LOCK } else { egui_phosphor::regular::LOCK_OPEN };
        if ui.add(egui::Button::new(RichText::new(format!("{} SYNC", sync_icon)).size(9.0).strong()).fill(if is_synced { app.theme.accent.linear_multiply(0.2) } else { app.theme.bg_inset })).on_hover_text("Sync Master Clock").clicked() {
            app.composer.sync_with_master_transport = !app.composer.sync_with_master_transport;
        }

        ui.add_space(app.theme.space_md);

        ui.label(RichText::new("BPM").strong().size(app.theme.type_caption).color(app.theme.text_secondary));
        let mut bpm = app.decks.global_bpm;
        if ui.add(egui::DragValue::new(&mut bpm).speed(0.1).range(20.0..=300.0)).changed() {
            app.decks.global_bpm = bpm;
            let _ = app.command_sender.send(Command::Core(CoreCommand::SetBpm(bpm)));
        }

        ui.add_space(app.theme.space_md);

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
                for sub in 0..16 {
                    app.composer.subchannel_sequencer_grid[trk][sub].resize(target_steps, 0.0);
                }
            }
        }

        ui.add_space(app.theme.space_md);

        // Zoomable Beatgrid controls
        if ui.button(egui_phosphor::regular::MAGNIFYING_GLASS_MINUS).on_hover_text("Zoom Out").clicked() {
            app.composer.grid_zoom = (app.composer.grid_zoom - 0.25).max(0.5);
        }
        ui.add(egui::Slider::new(&mut app.composer.grid_zoom, 0.5..=3.0).show_value(false));
        if ui.button(egui_phosphor::regular::MAGNIFYING_GLASS_PLUS).on_hover_text("Zoom In").clicked() {
            app.composer.grid_zoom = (app.composer.grid_zoom + 0.25).min(3.0);
        }

        ui.add_space(app.theme.space_md);

        let is_kbd_open = app.composer.keyboard_grid.is_open;
        if ui.add(egui::Button::new(RichText::new(format!("{} KBD", egui_phosphor::regular::PIANO_KEYS)).size(9.0).strong()).fill(if is_kbd_open { app.theme.accent } else { app.theme.bg_inset })).on_hover_text("Toggle Keyboard").clicked() {
            app.composer.keyboard_grid.is_open = !app.composer.keyboard_grid.is_open;
        }

        ui.add_space(app.theme.space_xs);

        if ui.button(RichText::new("CLEAR").size(9.0).strong()).on_hover_text("Clear All Patterns").clicked() {
            for i in 0..16 {
                 let _ = app.command_sender.send(Command::Performance(PerformanceCommand::ClearTrackPattern { node_idx: seq_node, track_idx: i as u32 }));
                 app.composer.studio_sequencer_grid[i].fill(0.0);
                 for sub in 0..16 {
                     app.composer.subchannel_sequencer_grid[i][sub].fill(0.0);
                 }
            }
        }
    });
    ui.add_space(app.theme.space_sm);

    // Flat 1:1 Arrangement Grid Frame
    Frame::none()
        .fill(app.theme.bg_dark)
        .stroke(app.theme.border_stroke)
        .rounding(Rounding::same(app.theme.radius_md))
        .inner_margin(Margin::same(app.theme.space_sm))
        .show(ui, |ui| {
            let mut extend_grid = false;
            let steps_count = app.composer.studio_sequencer_grid[0].len();
            let slot_w = (40.0 * app.composer.grid_zoom).clamp(15.0, 120.0);
            let slot_h = 42.0; // Compact flat track height
            let num_active_channels = app.mixer.num_channels.clamp(1, 16);

            ui.horizontal(|ui| {
                // LEFT SIDE: Decoupled Studio Track Headers column
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add_space(32.0); // Exact match for timeline header

                    let pad_labels = [
                        "01 KICK", "02 SNARE", "03 HH-CL", "04 HH-OP",
                        "05 TOM-LO", "06 TOM-MID", "07 TOM-HI", "08 PERC 1",
                        "09 PERC 2", "10 CLAP", "11 RIDE", "12 CRASH",
                        "13 FX 1", "14 FX 2", "15 AUX 1", "16 AUX 2",
                    ];

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
                            .inner_margin(Margin::symmetric(4.0, 2.0))
                            .show(ui, |ui| {
                                ui.set_width(175.0);
                                ui.set_height(slot_h - 4.0);
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        let (swatch_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
                                        ui.painter().rect_filled(swatch_rect, Rounding::same(1.5), track_color);
                                        ui.add_space(2.0);

                                        let expand_icon = if is_selected { "▼ " } else { "▸ " };
                                        ui.label(RichText::new(format!("{}TRK {:02}", expand_icon, track_idx + 1)).strong().size(app.theme.type_caption).color(app.theme.text_primary));

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let mute_color = if is_muted { app.theme.danger } else { app.theme.bg_inset };
                                            if ui.add_sized([16.0, 16.0], egui::Button::new(RichText::new("M").size(8.0).strong()).fill(mute_color)).clicked() {
                                                app.composer.track_mutes[track_idx] = !is_muted;
                                            }

                                            let solo_color = if is_solo { app.theme.warning } else { app.theme.bg_inset };
                                            if ui.add_sized([16.0, 16.0], egui::Button::new(RichText::new("S").size(8.0).strong()).fill(solo_color)).clicked() {
                                                app.composer.track_solos[track_idx] = !is_solo;
                                            }
                                        });
                                    });

                                    ui.add_space(1.0);

                                    // Sample / Instrument Picker dropdown
                                    let src_id = app.composer.track_sources[track_idx];
                                    let cached_track = src_id.and_then(|id| app.get_cached_track(id));
                                    let sample_label = cached_track.as_ref()
                                        .map(|t| format!("♪ {}", t.title))
                                        .unwrap_or_else(|| "⊕ SAMPLE / INSTRUMENT".to_string());

                                    egui::ComboBox::from_id_source(format!("seq_studio_src_{}", track_idx))
                                        .width(165.0)
                                        .selected_text(RichText::new(&sample_label).size(8.5).strong())
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
                                });
                            });

                        let rect = inner_resp.response.rect;
                        let response = ui.interact(rect, ui.make_persistent_id(format!("trk_hdr_studio_{}", track_idx)), Sense::click());
                        if response.clicked() {
                            if app.composer.selected_composer_track == Some(track_idx) {
                                app.composer.selected_composer_track = None;
                                app.mixer.folded_pads[track_idx] = false;
                            } else {
                                app.composer.selected_composer_track = Some(track_idx);
                                app.mixer.folded_pads[track_idx] = true;
                                app.mixer.focused_detail_channel = track_idx;
                            }
                        }

                        // ACCORDION EXPANSION: Render subchannel/pad headers directly below selected track
                        if is_selected {
                            ui.add_space(2.0);
                            for pad_i in 0..16 {
                                ui.allocate_ui_with_layout(Vec2::new(175.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    let (rect, _) = ui.allocate_exact_size(Vec2::new(175.0, 20.0), Sense::hover());
                                    ui.painter().rect_filled(rect, Rounding::same(2.0), app.theme.bg_inset);
                                    ui.painter().text(
                                        Pos2::new(rect.min.x + 8.0, rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        format!("↳ {}", pad_labels[pad_i]),
                                        egui::FontId::new(9.0, egui::FontFamily::Proportional),
                                        track_color,
                                    );
                                });
                                ui.add_space(1.0);
                            }
                        }

                        if track_idx < num_active_channels - 1 {
                            ui.add_space(4.0);
                        }
                    }
                });

                ui.add_space(4.0);

                // RIGHT SIDE: Timeline Header + Flat Pattern Clip Waveform Grid
                ScrollArea::horizontal()
                    .id_source("composer_flat_grid_scroll_h")
                    .show(ui, |ui| {
                        let mut grid_top_pos = Pos2::ZERO;
                        let mut grid_bottom_pos = Pos2::ZERO;
                        let mut first_slot_x = 0.0f32;

                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;

                            let steps_per_bar = app.composer.grid_step_resolution.clamp(16, 64);
                            let steps_per_beat = (steps_per_bar / 4).max(1);

                            let header_resp = ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                                for slot_idx in 0..steps_count {
                                    if slot_idx > 0 && slot_idx % steps_per_beat == 0 {
                                        ui.add_space(4.0);
                                    }
                                    let (rect, response) = ui.allocate_exact_size(Vec2::new(slot_w, 24.0), Sense::click());

                                    if slot_idx == 0 {
                                        first_slot_x = rect.min.x;
                                    }

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

                            // Render flat active channel rows in step grid
                            for track_idx in 0..num_active_channels {
                                let track_color = crate::InspectorApp::deck_color(&app.theme, track_idx % 4);
                                let is_muted = app.composer.track_mutes[track_idx];
                                let is_selected = app.composer.selected_composer_track == Some(track_idx);

                                let src_id = app.composer.track_sources[track_idx];
                                let cached_track = src_id.and_then(|id| app.get_cached_track(id));

                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);

                                    for slot_idx in 0..steps_count {
                                        if slot_idx > 0 && slot_idx % steps_per_beat == 0 {
                                            ui.add_space(4.0);
                                        }
                                        if slot_idx > 0 && slot_idx % steps_per_bar == 0 {
                                            ui.add_space(6.0);
                                        }

                                        let (rect, response) = ui.allocate_exact_size(Vec2::new(slot_w, slot_h), Sense::click());

                                        if slot_idx == steps_count - 1 && ui.is_rect_visible(rect) && steps_count < 512 {
                                            extend_grid = true;
                                        }

                                        let velocity = app.composer.studio_sequencer_grid[track_idx][slot_idx];

                                        let mut bg_color = if velocity > 0.0 {
                                            if is_muted {
                                                app.theme.bg_inset
                                            } else {
                                                track_color.gamma_multiply(0.35)
                                            }
                                        } else {
                                            track_color.gamma_multiply(0.03)
                                        };

                                        if app.composer.composer_playing && slot_idx == app.composer.sequencer_active_step {
                                            bg_color = track_color.gamma_multiply(0.6);
                                        }

                                        ui.painter().rect_filled(rect, Rounding::same(2.0), bg_color);
                                        let border_stroke = if velocity > 0.0 {
                                            Stroke::new(1.0_f32, track_color)
                                        } else {
                                            app.theme.border_stroke
                                        };
                                        ui.painter().rect_stroke(rect, Rounding::same(2.0), border_stroke);

                                        // Render mini audio waveforms or clip blocks
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
                                            app.composer.selected_composer_track = Some(track_idx);
                                            app.composer.selected_clip = Some((track_idx, slot_idx));

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

                                // ACCORDION EXPANSION: Render subchannel/pad step matrix directly below selected track
                                if is_selected {
                                    ui.add_space(2.0);
                                    for pad_i in 0..16 {
                                        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                            ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                                            for slot_idx in 0..steps_count {
                                                if slot_idx > 0 && slot_idx % steps_per_beat == 0 {
                                                    ui.add_space(4.0);
                                                }
                                                if slot_idx > 0 && slot_idx % steps_per_bar == 0 {
                                                    ui.add_space(6.0);
                                                }

                                                let (rect, response) = ui.allocate_exact_size(Vec2::new(slot_w, 20.0), Sense::click());
                                                let vel = app.composer.subchannel_sequencer_grid[track_idx][pad_i][slot_idx];

                                                let bg = if vel > 0.0 {
                                                    track_color.gamma_multiply(0.45)
                                                } else {
                                                    app.theme.bg_inset
                                                };

                                                ui.painter().rect_filled(rect, Rounding::same(2.0), bg);
                                                ui.painter().rect_stroke(rect, Rounding::same(2.0), Stroke::new(0.8_f32, if vel > 0.0 { track_color } else { app.theme.border_stroke.color }));

                                                if response.clicked() {
                                                    let is_on = app.composer.subchannel_sequencer_grid[track_idx][pad_i][slot_idx] == 0.0;
                                                    let val = if is_on { 1.0 } else { 0.0 };
                                                    app.composer.subchannel_sequencer_grid[track_idx][pad_i][slot_idx] = val;

                                                    let target_node = app.get_node_id("drum_machine_node").unwrap_or(70);
                                                    let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetSequencerStep {
                                                        node_idx: target_node,
                                                        track: pad_i as u32,
                                                        step: slot_idx as u32,
                                                        value: val,
                                                    }));
                                                }
                                            }
                                        });
                                        ui.add_space(1.0);
                                    }
                                }

                                if track_idx < num_active_channels - 1 {
                                    ui.add_space(4.0);
                                }
                            }

                            grid_bottom_pos = ui.cursor().left_top();

                            // Live Playhead Needle Drawing across the timeline
                            let live_beat = if app.composer.composer_playing {
                                telemetry.as_ref()
                                    .map(|t| t.beat_position as f32)
                                    .unwrap_or(app.composer.sequencer_active_step as f32)
                            } else {
                                0.0
                            };
                            let playhead_step = live_beat.max(0.0).min(steps_count as f32);
                            let bars_before = (playhead_step / 4.0).floor();
                            let playhead_x = first_slot_x + (playhead_step * slot_w) + (playhead_step * 2.0) + (bars_before * 4.0);

                            if playhead_x >= first_slot_x {
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
                    for sub in 0..16 {
                        app.composer.subchannel_sequencer_grid[i][sub].resize(steps_count + 16, 0.0);
                    }
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

pub fn render_clip_editor_drawer_panel(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    let selected_trk = app.composer.selected_composer_track.unwrap_or(0);
    let track_color = crate::InspectorApp::deck_color(&theme, selected_trk % 4);

    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("TRK {:02} CLIP & DRUM STEP MATRIX", selected_trk + 1)).strong().size(theme.type_caption).color(track_color));

        ui.add_space(theme.space_md);

        let src_id = app.composer.track_sources[selected_trk];
        let cached_track = src_id.and_then(|id| app.get_cached_track(id));
        let track_title = cached_track.as_ref().map(|t| t.title.as_str()).unwrap_or("No Sample / Instrument Loaded");
        ui.label(RichText::new(format!("♪ {}", track_title)).size(theme.type_caption).color(theme.text_secondary));
    });

    ui.add_space(theme.space_xs);
    ui.separator();
    ui.add_space(theme.space_xs);

    let steps_count = app.composer.studio_sequencer_grid[0].len();
    let steps_per_bar = app.composer.grid_step_resolution.clamp(16, 64);
    let steps_per_beat = (steps_per_bar / 4).max(1);
    let slot_w = (32.0 * app.composer.grid_zoom).clamp(14.0, 90.0);

    let pad_labels = [
        "01 KICK", "02 SNARE", "03 HH-CL", "04 HH-OP",
        "05 TOM-LO", "06 TOM-MID", "07 TOM-HI", "08 PERC 1",
        "09 PERC 2", "10 CLAP", "11 RIDE", "12 CRASH",
        "13 FX 1", "14 FX 2", "15 AUX 1", "16 AUX 2",
    ];

    ScrollArea::both()
        .id_source("bottom_drawer_clip_editor_scroll")
        .show(ui, |ui| {
            ui.vertical(|ui| {
                for pad_i in 0..16 {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);

                        // Pad Label & Sample Picker button
                        ui.allocate_ui_with_layout(Vec2::new(130.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(RichText::new(pad_labels[pad_i]).size(8.5).strong().color(track_color));

                            let sub_src = app.composer.subchannel_sources[selected_trk][pad_i];
                            let sub_track = sub_src.and_then(|id| app.get_cached_track(id));
                            let picker_text = sub_track.as_ref().map(|t| t.title.chars().take(8).collect::<String>()).unwrap_or_else(|| "⊕ Sample".into());

                            egui::ComboBox::from_id_source(format!("pad_picker_{}_{}", selected_trk, pad_i))
                                .width(65.0)
                                .selected_text(RichText::new(picker_text).size(7.5))
                                .show_ui(ui, |ui| {
                                    if ui.selectable_label(sub_src.is_none(), "(None)").clicked() {
                                        app.composer.subchannel_sources[selected_trk][pad_i] = None;
                                    }
                                    for lib_t in &app.library.cached_library_raw {
                                        if ui.selectable_label(sub_src == Some(lib_t.id), &lib_t.title).clicked() {
                                            app.composer.subchannel_sources[selected_trk][pad_i] = Some(lib_t.id);
                                        }
                                    }
                                });
                        });

                        ui.add_space(4.0);

                        // Pad Step Matrix (Scoped strictly to this parent track & pad)
                        for slot_idx in 0..steps_count {
                            if slot_idx > 0 && slot_idx % steps_per_beat == 0 {
                                ui.add_space(3.0);
                            }
                            if slot_idx > 0 && slot_idx % steps_per_bar == 0 {
                                ui.add_space(5.0);
                            }

                            let (rect, response) = ui.allocate_exact_size(Vec2::new(slot_w, 20.0), Sense::click());
                            let vel = app.composer.subchannel_sequencer_grid[selected_trk][pad_i][slot_idx];

                            let bg = if vel > 0.0 {
                                track_color.gamma_multiply(0.45)
                            } else {
                                theme.bg_inset
                            };

                            ui.painter().rect_filled(rect, Rounding::same(2.0), bg);
                            ui.painter().rect_stroke(rect, Rounding::same(2.0), Stroke::new(0.8_f32, if vel > 0.0 { track_color } else { theme.border_stroke.color }));

                            if response.clicked() {
                                let is_on = app.composer.subchannel_sequencer_grid[selected_trk][pad_i][slot_idx] == 0.0;
                                let val = if is_on { 1.0 } else { 0.0 };
                                app.composer.subchannel_sequencer_grid[selected_trk][pad_i][slot_idx] = val;

                                let target_node = app.get_node_id("drum_machine_node").unwrap_or(70);
                                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetSequencerStep {
                                    node_idx: target_node,
                                    track: pad_i as u32,
                                    step: slot_idx as u32,
                                    value: val,
                                }));
                            }
                        }
                    });
                    ui.add_space(1.0);
                }
            });
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
            active_bottom_drawer: None,
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
