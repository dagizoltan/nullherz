use egui::{Ui, Vec2, Sense, RichText, Frame, Margin, Stroke, Color32, Align2, FontId, Rect, pos2};
use nullherz_ui_hal::Theme;
use crate::InspectorApp;

/// Render a studio icon button card for the operations grid
fn render_op_icon_button(
    ui: &mut Ui,
    theme: &Theme,
    icon: &str,
    title: &str,
    subtitle: &str,
    accent_color: Color32,
    enabled: bool,
) -> egui::Response {
    let card_width = ((ui.available_width() - 18.0) / 4.0).max(110.0);
    let card_height = 56.0;

    let (rect, response) = ui.allocate_exact_size(Vec2::new(card_width, card_height), Sense::click());

    let bg_color = if !enabled {
        theme.bg_inset.linear_multiply(0.5)
    } else if response.is_pointer_button_down_on() {
        accent_color.linear_multiply(0.25)
    } else if response.hovered() {
        theme.bg_surface_raised
    } else {
        theme.bg_inset
    };

    let border_stroke = if response.hovered() && enabled {
        Stroke::new(1.5, accent_color)
    } else {
        Stroke::new(1.0, theme.border_stroke.color)
    };

    ui.painter().rect_filled(rect, theme.radius_sm, bg_color);
    ui.painter().rect_stroke(rect, theme.radius_sm, border_stroke);

    let icon_color = if enabled { accent_color } else { theme.text_secondary.linear_multiply(0.4) };
    let text_color = if enabled { theme.text_primary } else { theme.text_secondary.linear_multiply(0.4) };

    let content_rect = rect.shrink(6.0);
    let mut child_ui = ui.child_ui(content_rect, egui::Layout::left_to_right(egui::Align::Center), None);

    child_ui.horizontal(|ui| {
        ui.label(RichText::new(icon).size(20.0).color(icon_color));
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(title).strong().size(theme.type_caption).color(text_color));
            ui.label(RichText::new(subtitle).size(8.5).color(theme.text_secondary));
        });
    });

    response
}

/// Render the Studio Audio Editor page
pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    if let Some(track_id) = app.library.selected_library_track {
        if let Some(track) = app.get_cached_track(track_id) {
            let total_samples = track.metadata.total_samples;
            let sample_rate = track.metadata.sample_rate.max(1);
            let total_duration_sec = total_samples as f64 / sample_rate as f64;
            let ext = std::path::Path::new(&track.path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("WAV")
                .to_uppercase();

            // --- 1. STUDIO HEADER & TRACK METADATA BANNER ---
            Frame::none()
                .fill(theme.bg_surface)
                .rounding(theme.radius_md)
                .stroke(Stroke::new(1.0, theme.border))
                .inner_margin(Margin::same(theme.space_md))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&track.title).strong().size(theme.type_heading).color(theme.accent));
                            ui.add_space(theme.space_xs);
                            ui.label(RichText::new(format!("by {}", track.artist)).size(theme.type_body).color(theme.text_secondary));

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let deselect_btn = egui::Button::new(
                                    RichText::new(format!("{} DESELECT", egui_phosphor::regular::X))
                                        .size(theme.type_caption)
                                        .strong()
                                        .color(theme.text_secondary)
                                ).fill(theme.bg_inset);
                                if ui.add(deselect_btn).clicked() {
                                    app.library.selected_library_track = None;
                                    return;
                                }

                                ui.add_space(theme.space_sm);

                                // Deck Quick Load buttons
                                for deck_idx in 0..4 {
                                    let deck_char = (b'A' + deck_idx as u8) as char;
                                    let deck_color = theme.deck_colors[deck_idx];
                                    let btn = egui::Button::new(
                                        RichText::new(format!("LOAD {}", deck_char))
                                            .size(theme.type_caption)
                                            .strong()
                                            .color(deck_color)
                                    ).fill(theme.bg_inset).stroke(Stroke::new(1.0, deck_color.linear_multiply(0.5)));

                                    if ui.add(btn).on_hover_text(format!("Load track into Deck {}", deck_char)).clicked() {
                                        let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                                            nullherz_traits::PerformanceCommand::LoadTrackToDeck {
                                                deck_id: deck_char,
                                                sample_id: track.id,
                                            }
                                        ));
                                    }
                                }
                            });
                        });

                        ui.add_space(theme.space_xs);

                        // Metadata Format Badges & Properties Row
                        ui.horizontal(|ui| {
                            // Format Badge
                            Frame::none()
                                .fill(theme.accent.linear_multiply(0.15))
                                .rounding(theme.radius_sm)
                                .stroke(Stroke::new(1.0, theme.accent))
                                .inner_margin(Margin::symmetric(6.0, 2.0))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(&ext).strong().size(10.0).color(theme.accent));
                                });

                            ui.add_space(4.0);

                            // Sample Rate Badge
                            Frame::none()
                                .fill(theme.bg_inset)
                                .rounding(theme.radius_sm)
                                .stroke(Stroke::new(1.0, theme.border_stroke.color))
                                .inner_margin(Margin::symmetric(6.0, 2.0))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(format!("{:.1} kHz", sample_rate as f32 / 1000.0)).size(10.0).color(theme.text_primary));
                                });

                            ui.add_space(4.0);

                            // Channels Badge
                            Frame::none()
                                .fill(theme.bg_inset)
                                .rounding(theme.radius_sm)
                                .stroke(Stroke::new(1.0, theme.border_stroke.color))
                                .inner_margin(Margin::symmetric(6.0, 2.0))
                                .show(ui, |ui| {
                                    let ch_str = if track.metadata.channels == 1 { "Mono 1ch" } else { "Stereo 2ch" };
                                    ui.label(RichText::new(ch_str).size(10.0).color(theme.text_primary));
                                });

                            ui.add_space(4.0);

                            // Duration Badge
                            Frame::none()
                                .fill(theme.bg_inset)
                                .rounding(theme.radius_sm)
                                .stroke(Stroke::new(1.0, theme.border_stroke.color))
                                .inner_margin(Margin::symmetric(6.0, 2.0))
                                .show(ui, |ui| {
                                    let mins = (total_duration_sec / 60.0) as u32;
                                    let secs = (total_duration_sec % 60.0) as u32;
                                    let ms = ((total_duration_sec % 1.0) * 1000.0) as u32;
                                    ui.label(RichText::new(format!("{:02}:{:02}.{:03}", mins, secs, ms)).size(10.0).color(theme.text_primary));
                                });

                            ui.add_space(4.0);

                            // BPM Badge
                            Frame::none()
                                .fill(theme.bg_inset)
                                .rounding(theme.radius_sm)
                                .stroke(Stroke::new(1.0, theme.border_stroke.color))
                                .inner_margin(Margin::symmetric(6.0, 2.0))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(format!("BPM: {:.2}", track.metadata.bpm)).size(10.0).color(theme.success));
                                });

                            ui.add_space(4.0);

                            // Root Key Badge
                            Frame::none()
                                .fill(theme.bg_inset)
                                .rounding(theme.radius_sm)
                                .stroke(Stroke::new(1.0, theme.border_stroke.color))
                                .inner_margin(Margin::symmetric(6.0, 2.0))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(format!("KEY: {:?}", track.metadata.root_key)).size(10.0).color(theme.warning));
                                });

                            ui.add_space(theme.space_sm);
                            ui.label(RichText::new(&track.path).size(theme.type_caption).color(theme.text_secondary));
                        });
                    });
                });

            ui.add_space(theme.space_sm);

            // Compute visible window derived from editor zoom and scroll
            let zoom = app.editor.editor_waveform_zoom.clamp(1.0, 32.0);
            let win_span = 1.0 / zoom;
            let max_scroll = (1.0 - win_span).max(0.0);
            let scroll = app.editor.editor_waveform_scroll.clamp(0.0, max_scroll);
            let start_ratio = scroll;
            let end_ratio = (scroll + win_span).min(1.0);

            // --- 2. TRANSPORT & TIMECODE CONTROLS BAR ---
            Frame::none()
                .fill(theme.bg_surface)
                .rounding(theme.radius_sm)
                .stroke(Stroke::new(1.0, theme.border))
                .inner_margin(Margin::symmetric(theme.space_md, theme.space_xs))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Play / Pause Toggle
                        let play_icon = if app.editor.editor_is_playing {
                            egui_phosphor::regular::PAUSE
                        } else {
                            egui_phosphor::regular::PLAY
                        };
                        let play_bg = if app.editor.editor_is_playing { theme.accent } else { theme.bg_inset };
                        let play_fg = if app.editor.editor_is_playing { Color32::BLACK } else { theme.accent };

                        if ui.add_sized([32.0, 24.0], egui::Button::new(RichText::new(play_icon).size(14.0).strong().color(play_fg)).fill(play_bg)).clicked() {
                            app.editor.editor_is_playing = !app.editor.editor_is_playing;
                        }

                        // Stop Button
                        if ui.add_sized([32.0, 24.0], egui::Button::new(RichText::new(egui_phosphor::regular::SQUARE).size(12.0).strong()).fill(theme.bg_inset)).on_hover_text("Stop & Return Playhead").clicked() {
                            app.editor.editor_is_playing = false;
                            if let Some((s, _)) = app.editor.editor_selection {
                                app.editor.editor_playhead_pos = s.min(1.0);
                            } else {
                                app.editor.editor_playhead_pos = 0.0;
                            }
                        }

                        // Loop Toggle Button
                        let loop_bg = if app.editor.editor_is_looping { theme.warning } else { theme.bg_inset };
                        let loop_fg = if app.editor.editor_is_looping { Color32::BLACK } else { theme.text_secondary };
                        if ui.add_sized([32.0, 24.0], egui::Button::new(RichText::new(egui_phosphor::regular::REPEAT).size(12.0).strong().color(loop_fg)).fill(loop_bg)).on_hover_text("Toggle Region Loop").clicked() {
                            app.editor.editor_is_looping = !app.editor.editor_is_looping;
                        }

                        ui.add_space(theme.space_sm);

                        // Timecode Readout Display
                        let curr_sec = app.editor.editor_playhead_pos as f64 * total_duration_sec;
                        let curr_m = (curr_sec / 60.0) as u32;
                        let curr_s = (curr_sec % 60.0) as u32;
                        let curr_ms = ((curr_sec % 1.0) * 100.0) as u32;

                        let tot_m = (total_duration_sec / 60.0) as u32;
                        let tot_s = (total_duration_sec % 60.0) as u32;
                        let tot_ms = ((total_duration_sec % 1.0) * 100.0) as u32;

                        ui.label(RichText::new(format!("{:02}:{:02}.{:02} / {:02}:{:02}.{:02}", curr_m, curr_s, curr_ms, tot_m, tot_s, tot_ms))
                            .monospace().strong().size(theme.type_body).color(theme.accent));

                        ui.add_space(theme.space_md);

                        // Zoom & Scroll Controls
                        ui.label(RichText::new("ZOOM").size(theme.type_caption).strong());
                        ui.add(egui::Slider::new(&mut app.editor.editor_waveform_zoom, 1.0..=32.0).logarithmic(true).show_value(false));

                        ui.add_space(theme.space_xs);
                        ui.label(RichText::new("SCROLL").size(theme.type_caption).strong());
                        ui.add_enabled(max_scroll > 0.0, egui::Slider::new(&mut app.editor.editor_waveform_scroll, 0.0..=max_scroll).show_value(false));

                        ui.add_space(theme.space_xs);
                        if ui.button(RichText::new(format!("{} RESET", egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)).size(theme.type_caption)).clicked() {
                            app.editor.editor_waveform_zoom = 1.0;
                            app.editor.editor_waveform_scroll = 0.0;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Waveform Style Selector
                            let curr_style = app.mixer.waveform_styles[0];
                            for style in nullherz_ui_hal::render::waveform_renderer::WaveformStyle::all().iter().rev() {
                                let is_sel = curr_style == *style;
                                if ui.selectable_label(is_sel, style.name()).clicked() {
                                    app.mixer.waveform_styles[0] = *style;
                                }
                            }
                            ui.label(RichText::new("STYLE:").size(theme.type_caption).strong().color(theme.accent));
                        });
                    });
                });

            ui.add_space(theme.space_xs);

            // Playhead advancing step when playing
            if app.editor.editor_is_playing {
                let frame_dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.050) as f64;
                let dt_ratio = frame_dt / total_duration_sec.max(0.1);
                app.editor.editor_playhead_pos = (app.editor.editor_playhead_pos + dt_ratio as f32).min(1.0);

                if let Some((s, e)) = app.editor.editor_selection {
                    let s_min = s.min(e);
                    let s_max = s.max(e);
                    if app.editor.editor_is_looping && app.editor.editor_playhead_pos >= s_max {
                        app.editor.editor_playhead_pos = s_min;
                    } else if app.editor.editor_playhead_pos >= 1.0 {
                        app.editor.editor_is_playing = false;
                        app.editor.editor_playhead_pos = 0.0;
                    }
                } else if app.editor.editor_playhead_pos >= 1.0 {
                    app.editor.editor_is_playing = false;
                    app.editor.editor_playhead_pos = 0.0;
                }
                ui.ctx().request_repaint();
            }

            // --- 3. FULL-TRACK OVERVIEW MINIMAP ---
            ui.horizontal(|ui| {
                ui.label(RichText::new("FULL-TRACK OVERVIEW MINIMAP").strong().size(theme.type_caption).color(theme.accent));
            });
            let minimap_size = Vec2::new(ui.available_width(), 36.0);
            let (m_rect, m_response) = ui.allocate_at_least(minimap_size, Sense::click_and_drag());
            ui.painter().rect_filled(m_rect, theme.radius_sm, theme.bg_dark);
            ui.painter().rect_stroke(m_rect, theme.radius_sm, theme.border_stroke);

            // Render full-length peak envelope on minimap
            let peaks = track.metadata.peaks.as_slice();
            if !peaks.is_empty() {
                let num_peaks = peaks.len();
                let step_w = m_rect.width() / num_peaks as f32;
                let center_y = m_rect.center().y;
                let half_h = m_rect.height() * 0.45;

                for (i, &amp) in peaks.iter().enumerate() {
                    let x = m_rect.left() + i as f32 * step_w;
                    let h = (amp.abs() * half_h).clamp(1.0, half_h);
                    ui.painter().line_segment(
                        [pos2(x, center_y - h), pos2(x, center_y + h)],
                        Stroke::new(1.0_f32, theme.text_secondary.linear_multiply(0.4)),
                    );
                }
            }

            // Draw minimap viewport rectangle representing zoomed window
            let viewport_left = m_rect.left() + start_ratio * m_rect.width();
            let viewport_right = m_rect.left() + end_ratio * m_rect.width();
            let vp_rect = Rect::from_min_max(pos2(viewport_left, m_rect.top()), pos2(viewport_right, m_rect.bottom()));
            ui.painter().rect_filled(vp_rect, 0.0, theme.accent.linear_multiply(0.25));
            ui.painter().rect_stroke(vp_rect, 0.0, Stroke::new(1.5, theme.accent));

            // Render playhead indicator line on minimap
            let playhead_x = m_rect.left() + app.editor.editor_playhead_pos * m_rect.width();
            ui.painter().line_segment(
                [pos2(playhead_x, m_rect.top()), pos2(playhead_x, m_rect.bottom())],
                Stroke::new(2.0, theme.warning),
            );

            // Interactive Minimap Viewport Dragging / Clicking
            if m_response.clicked() || m_response.dragged() {
                if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
                    let norm_x = ((pos.x - m_rect.left()) / m_rect.width()).clamp(0.0, 1.0);
                    let target_scroll = (norm_x - win_span * 0.5).clamp(0.0, max_scroll);
                    app.editor.editor_waveform_scroll = target_scroll;
                    app.editor.editor_playhead_pos = norm_x;
                }
            }

            ui.add_space(theme.space_xs);

            // --- 4. PRIMARY WAVEFORM EDITOR CANVAS WITH TIME RULER ---
            let main_canvas_height = 210.0;
            let (rect, response) = ui.allocate_at_least(Vec2::new(ui.available_width(), main_canvas_height), Sense::click_and_drag());
            ui.painter().rect_filled(rect, theme.radius_md, theme.bg_dark);
            ui.painter().rect_stroke(rect, theme.radius_md, theme.border_stroke);

            // Render Time Ruler at Top of Waveform Canvas
            let ruler_height = 20.0;
            let ruler_rect = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + ruler_height));
            ui.painter().rect_filled(ruler_rect, theme.radius_sm, theme.bg_surface);
            ui.painter().line_segment(
                [pos2(rect.left(), rect.min.y + ruler_height), pos2(rect.right(), rect.min.y + ruler_height)],
                Stroke::new(1.0, theme.border_stroke.color),
            );

            // Draw time ticks across ruler
            let num_ticks = 10;
            let tick_step = win_span / num_ticks as f32;
            for t_idx in 0..=num_ticks {
                let ratio = start_ratio + t_idx as f32 * tick_step;
                let x = rect.left() + (t_idx as f32 / num_ticks as f32) * rect.width();
                let time_sec = ratio as f64 * total_duration_sec;
                let m = (time_sec / 60.0) as u32;
                let s = (time_sec % 60.0) as u32;
                let ms = ((time_sec % 1.0) * 10.0) as u32;

                ui.painter().line_segment(
                    [pos2(x, rect.min.y + ruler_height - 6.0), pos2(x, rect.min.y + ruler_height)],
                    Stroke::new(1.0, theme.text_secondary),
                );

                ui.painter().text(
                    pos2(x + 2.0, rect.min.y + 2.0),
                    Align2::LEFT_TOP,
                    format!("{:02}:{:02}.{}", m, s, ms),
                    FontId::proportional(9.0),
                    theme.text_secondary,
                );
            }

            // Waveform Sub-Rect under Time Ruler
            let wf_rect = Rect::from_min_max(pos2(rect.min.x, rect.min.y + ruler_height), rect.max);

            // Mouse wheel zoom support on main waveform
            if response.hovered() {
                let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
                if scroll_delta != 0.0 {
                    let zoom_factor = if scroll_delta > 0.0 { 1.15 } else { 0.85 };
                    let new_zoom = (app.editor.editor_waveform_zoom * zoom_factor).clamp(1.0, 32.0);
                    app.editor.editor_waveform_zoom = new_zoom;
                }
            }

            if response.dragged() {
                let current_pos = ui.input(|i| i.pointer.latest_pos()).unwrap_or(pos2(0.0, 0.0));
                let x_norm = ((current_pos.x - wf_rect.left()) / wf_rect.width()).clamp(0.0, 1.0);
                let track_ratio = (start_ratio + x_norm * win_span).clamp(0.0, 1.0);

                if let Some((s_start, _)) = app.editor.editor_selection {
                    app.editor.editor_selection = Some((s_start, track_ratio));
                } else {
                    app.editor.editor_selection = Some((track_ratio, track_ratio));
                }
            }
            if response.clicked() {
                let current_pos = ui.input(|i| i.pointer.latest_pos()).unwrap_or(pos2(0.0, 0.0));
                let x_norm = ((current_pos.x - wf_rect.left()) / wf_rect.width()).clamp(0.0, 1.0);
                let track_ratio = (start_ratio + x_norm * win_span).clamp(0.0, 1.0);
                app.editor.editor_selection = Some((track_ratio, track_ratio));
                app.editor.editor_playhead_pos = track_ratio;
            }

            // Draw selection overlay following zoom window
            if let Some((start, end)) = app.editor.editor_selection {
                let s_min = start.min(end);
                let s_max = start.max(end);

                let norm_start = (s_min - start_ratio) / win_span;
                let norm_end = (s_max - start_ratio) / win_span;

                let left = wf_rect.left() + norm_start.clamp(0.0, 1.0) * wf_rect.width();
                let right = wf_rect.left() + norm_end.clamp(0.0, 1.0) * wf_rect.width();

                if right > left {
                    let sel_rect = Rect::from_min_max(pos2(left, wf_rect.top()), pos2(right, wf_rect.bottom()));
                    ui.painter().rect_filled(sel_rect, 0.0, theme.accent.linear_multiply(0.2));
                    ui.painter().rect_stroke(sel_rect, 0.0, Stroke::new(1.0, theme.accent));
                }
            }

            // Render windowed waveform (exact zoomed slice)
            if let Some(wf_lock) = &app.waveform_renderer {
                let mut wf = wf_lock.lock();
                let color = theme.accent.to_array().map(|v| v as f32 / 255.0);

                if let Some(wgpu) = &app.wgpu_renderer {
                    let wgpu = wgpu.lock();
                    let style = app.mixer.waveform_styles[0];
                    wf.update_globals(&wgpu.queue, 0.0, 1.0, false, style, color);
                    if track.metadata.band_waveform.is_empty() {
                        wf.update_from_mip_window(&wgpu.queue, &track.metadata.mip_waveform, start_ratio, end_ratio, wf_rect.width() as u32, color);
                    } else {
                        wf.update_from_band_window(&wgpu.queue, &track.metadata.band_waveform, start_ratio, end_ratio, wf_rect.width() as u32, style, color, [1.0, 1.0, 1.0]);
                    }
                }

                nullherz_ui_hal::render::waveform_renderer::ui_paint_waveform(ui, wf_rect, wf_lock.clone());
            }

            // Draw visual transient chop marks following zoom & scroll
            if total_samples > 0 {
                let transient_stroke = Stroke::new(1.5_f32, theme.success.linear_multiply(0.8));
                for &t in track.metadata.transients.iter() {
                    let t_ratio = t as f32 / total_samples as f32;
                    let norm_x = (t_ratio - start_ratio) / win_span;
                    if (0.0..=1.0).contains(&norm_x) {
                        let x = wf_rect.left() + norm_x * wf_rect.width();
                        ui.painter().line_segment(
                            [pos2(x, wf_rect.top()), pos2(x, wf_rect.bottom())],
                            transient_stroke,
                        );
                    }
                }
            }

            // Render vertical playhead needle line on main waveform canvas
            let norm_playhead_x = (app.editor.editor_playhead_pos - start_ratio) / win_span;
            if (0.0..=1.0).contains(&norm_playhead_x) {
                let playhead_x = wf_rect.left() + norm_playhead_x * wf_rect.width();
                ui.painter().line_segment(
                    [pos2(playhead_x, rect.top()), pos2(playhead_x, rect.bottom())],
                    Stroke::new(2.0, theme.warning),
                );
            }

            ui.add_space(theme.space_xs);

            // --- 5. SELECTION STATS & REGION MANAGEMENT BANNER ---
            Frame::none()
                .fill(theme.bg_inset)
                .rounding(theme.radius_sm)
                .stroke(theme.border_stroke)
                .inner_margin(Margin::symmetric(theme.space_md, theme.space_xs))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some((s, e)) = app.editor.editor_selection {
                            let (start, end) = if s < e { (s, e) } else { (e, s) };
                            let total_s = track.metadata.total_samples as f64;
                            let sr = track.metadata.sample_rate.max(1) as f64;

                            let start_samp = (start as f64 * total_s) as u64;
                            let end_samp = (end as f64 * total_s) as u64;
                            let len_samp = end_samp.saturating_sub(start_samp);

                            let start_sec = start_samp as f64 / sr;
                            let len_sec = len_samp as f64 / sr;
                            let beats = (len_sec * (track.metadata.bpm as f64 / 60.0)).max(0.0);

                            ui.label(RichText::new("SELECTION:").strong().size(theme.type_caption).color(theme.accent));
                            ui.label(RichText::new(format!("{:.3}s - {:.3}s ({:.3}s / {:.2} beats | {} samples)",
                                start_sec, start_sec + len_sec, len_sec, beats, len_samp)).size(theme.type_caption));

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(RichText::new(format!("{} CLEAR", egui_phosphor::regular::X)).size(theme.type_caption)).clicked() {
                                    app.editor.editor_selection = None;
                                }
                                if ui.button(RichText::new(format!("{} INVERT", egui_phosphor::regular::ARROWS_OUT_LINE_HORIZONTAL)).size(theme.type_caption)).clicked() {
                                    app.editor.editor_selection = Some((end, 1.0));
                                }
                                if ui.button(RichText::new(format!("{} SELECT ALL", egui_phosphor::regular::SELECTION_ALL)).size(theme.type_caption)).clicked() {
                                    app.editor.editor_selection = Some((0.0, 1.0));
                                }
                            });
                        } else {
                            ui.label(RichText::new("SELECTION: None (Click or drag on waveform to define region)").size(theme.type_caption).color(theme.text_secondary));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(RichText::new(format!("{} SELECT ALL", egui_phosphor::regular::SELECTION_ALL)).size(theme.type_caption)).clicked() {
                                    app.editor.editor_selection = Some((0.0, 1.0));
                                }
                            });
                        }
                    });
                });

            ui.add_space(theme.space_sm);

            // --- 6. HIGH-DENSITY STUDIO OPERATIONS ICON GRID & PARAMETER BARS ---
            Frame::none()
                .fill(theme.bg_surface)
                .rounding(theme.radius_md)
                .stroke(Stroke::new(1.0, theme.border))
                .inner_margin(Margin::same(theme.space_md))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} AUDIO OPERATIONS & DSP TOOLKIT", egui_phosphor::regular::SLIDERS_HORIZONTAL)).strong().size(theme.type_body).color(theme.accent));
                        });

                        ui.add_space(theme.space_xs);

                        let has_selection = app.editor.editor_selection.is_some();

                        // Row 1: Primary Sample Actions Icon Grid
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;

                            // 1. Crop
                            let crop_res = render_op_icon_button(
                                ui,
                                &theme,
                                egui_phosphor::regular::CROP,
                                "CROP REGION",
                                "Cut to selection",
                                theme.accent,
                                has_selection,
                            );
                            if crop_res.clicked() && has_selection {
                                if let Some((s, e)) = app.editor.editor_selection {
                                    let (start, end) = if s < e { (s, e) } else { (e, s) };
                                    let total_samples = track.metadata.total_samples as f32;
                                    let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::Crop {
                                        sample_id: track.id,
                                        start_samples: (start * total_samples) as u64,
                                        end_samples: (end * total_samples) as u64,
                                    }));
                                }
                            }

                            // 2. Peak Normalize
                            let norm_res = render_op_icon_button(
                                ui,
                                &theme,
                                egui_phosphor::regular::LIGHTNING,
                                "NORMALIZE",
                                "0 dB Peak Gain",
                                theme.accent,
                                true,
                            );
                            if norm_res.clicked() {
                                let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::Normalize { sample_id: track.id }));
                            }

                            // 3. Chop Transients
                            let chop_res = render_op_icon_button(
                                ui,
                                &theme,
                                egui_phosphor::regular::KNIFE,
                                "CHOP TRANSIENTS",
                                &format!("{} Marks", track.metadata.transients.len()),
                                theme.success,
                                true,
                            );
                            if chop_res.clicked() {
                                let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::ChopByTransient { sample_id: track.id }));
                            }

                            // 4. Time Stretch
                            let stretch_res = render_op_icon_button(
                                ui,
                                &theme,
                                egui_phosphor::regular::WAVEFORM,
                                "TIME STRETCH",
                                &format!("{:.2}x Ratio", app.editor.editor_time_stretch_ratio),
                                theme.warning,
                                true,
                            );
                            if stretch_res.clicked() {
                                let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::TimeStretch {
                                    sample_id: track.id,
                                    ratio: app.editor.editor_time_stretch_ratio,
                                }));
                            }
                        });

                        ui.add_space(6.0);

                        // Row 2: Secondary DSP Actions Icon Grid
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;

                            // 5. Reverse Sample
                            let rev_res = render_op_icon_button(
                                ui,
                                &theme,
                                egui_phosphor::regular::ARROWS_LEFT_RIGHT,
                                "REVERSE",
                                "Invert Buffer",
                                theme.accent,
                                true,
                            );
                            if rev_res.clicked() {
                                // Reverse action placeholder
                            }

                            // 6. Re-Analyze DNA
                            let dna_res = render_op_icon_button(
                                ui,
                                &theme,
                                egui_phosphor::regular::DNA,
                                "RE-ANALYZE DNA",
                                "SoundDNA Passport",
                                theme.accent,
                                true,
                            );
                            if dna_res.clicked() {
                                let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::ReAnalyze { sample_id: track.id }));
                            }

                            // 7. Fade In / Out
                            let fade_res = render_op_icon_button(
                                ui,
                                &theme,
                                egui_phosphor::regular::FLOPPY_DISK,
                                "FADE REGION",
                                "Smooth Envelopes",
                                theme.success,
                                has_selection,
                            );
                            if fade_res.clicked() {
                                // Fade region placeholder
                            }

                            // 8. Export Slices
                            let exp_res = render_op_icon_button(
                                ui,
                                &theme,
                                egui_phosphor::regular::EXPORT,
                                "EXPORT SLICES",
                                "Save to Library",
                                theme.warning,
                                true,
                            );
                            if exp_res.clicked() {
                                // Export slice action
                            }
                        });

                        ui.add_space(theme.space_sm);

                        // Row 3: Parameter Sliders Bar
                        Frame::none()
                            .fill(theme.bg_inset)
                            .rounding(theme.radius_sm)
                            .stroke(Stroke::new(1.0, theme.border_stroke.color))
                            .inner_margin(Margin::same(theme.space_sm))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Gain Trim
                                    ui.label(RichText::new("Gain Trim:").size(theme.type_caption).strong());
                                    ui.add(egui::Slider::new(&mut app.editor.editor_gain_trim, 0.0..=2.0).show_value(true));

                                    ui.add_space(theme.space_md);

                                    // Transient Sensitivity
                                    ui.label(RichText::new("Sensitivity:").size(theme.type_caption).strong());
                                    ui.add(egui::Slider::new(&mut app.editor.editor_transient_sensitivity, 0.0..=1.0).show_value(true));

                                    ui.add_space(theme.space_md);

                                    // Pitch Shift
                                    ui.label(RichText::new("Pitch (st):").size(theme.type_caption).strong());
                                    ui.add(egui::Slider::new(&mut app.editor.editor_pitch_shift_semitones, -12.0..=12.0).show_value(true));

                                    ui.add_space(theme.space_md);

                                    // Stretch Ratio
                                    ui.label(RichText::new("Stretch Ratio:").size(theme.type_caption).strong());
                                    ui.add(egui::Slider::new(&mut app.editor.editor_time_stretch_ratio, 0.5..=2.0).show_value(true));
                                });
                            });
                    });
                });

        } else {
            ui.label(RichText::new("Track not found in library.").color(theme.danger).size(theme.type_body));
            if ui.button(RichText::new("Deselect").size(theme.type_label)).clicked() { app.library.selected_library_track = None; }
        }
    } else {
        ui.vertical_centered(|ui| {
            ui.add_space(theme.space_xl * 3.0);
            ui.label(RichText::new("NO TRACK SELECTED").size(theme.type_heading).color(theme.text_secondary));
            ui.label(RichText::new("Select a track from the library to begin editing.").size(theme.type_body));
            ui.add_space(theme.space_md);
            if ui.button(RichText::new("OPEN LIBRARY").size(theme.type_label)).clicked() {
                app.active_right_tab = Some(crate::RightTab::Library);
            }
        });
    }
}
