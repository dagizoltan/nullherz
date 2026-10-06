use egui::{Ui, Vec2, Sense, RichText, Frame, Margin};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    if let Some(track_id) = app.library.selected_library_track {
        if let Some(track) = app.get_cached_track(track_id) {
            // Header Bar & Track Metadata Banner
            ui.horizontal(|ui| {
                ui.label(RichText::new(&track.title).strong().size(theme.type_heading));
                ui.label(RichText::new(format!("by {}", track.artist)).size(theme.type_body));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(RichText::new(format!("{} DESELECT TRACK", egui_phosphor::regular::X)).size(theme.type_caption)).clicked() {
                        app.library.selected_library_track = None;
                        return;
                    }
                });
            });
            ui.add_space(theme.space_xs);
            ui.label(RichText::new(&track.path).size(theme.type_caption).color(theme.text_secondary));

            ui.add_space(theme.space_sm);

            // Compute visible window derived from editor zoom and scroll
            let zoom = app.editor.editor_waveform_zoom.clamp(1.0, 32.0);
            let win_span = 1.0 / zoom;
            let max_scroll = (1.0 - win_span).max(0.0);
            let scroll = app.editor.editor_waveform_scroll.clamp(0.0, max_scroll);
            let start_ratio = scroll;
            let end_ratio = (scroll + win_span).min(1.0);

            // --- 1. FULL-TRACK OVERVIEW MINIMAP ---
            ui.horizontal(|ui| {
                ui.label(RichText::new("OVERVIEW MINIMAP").strong().size(theme.type_caption).color(theme.accent));
            });
            let minimap_size = Vec2::new(ui.available_width(), 32.0);
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
                        [egui::pos2(x, center_y - h), egui::pos2(x, center_y + h)],
                        egui::Stroke::new(1.0_f32, theme.text_secondary.linear_multiply(0.4)),
                    );
                }
            }

            // Draw minimap viewport rectangle representing zoomed window
            let viewport_left = m_rect.left() + start_ratio * m_rect.width();
            let viewport_right = m_rect.left() + end_ratio * m_rect.width();
            let vp_rect = egui::Rect::from_min_max(egui::pos2(viewport_left, m_rect.top()), egui::pos2(viewport_right, m_rect.bottom()));
            ui.painter().rect_filled(vp_rect, 0.0, theme.accent.linear_multiply(0.25));
            ui.painter().rect_stroke(vp_rect, 0.0, egui::Stroke::new(1.5, theme.accent));

            // Interactive Minimap Viewport Dragging / Clicking
            if m_response.clicked() || m_response.dragged() {
                if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
                    let norm_x = ((pos.x - m_rect.left()) / m_rect.width()).clamp(0.0, 1.0);
                    let target_scroll = (norm_x - win_span * 0.5).clamp(0.0, max_scroll);
                    app.editor.editor_waveform_scroll = target_scroll;
                }
            }

            ui.add_space(theme.space_xs);

            // --- 2. PRIMARY WAVEFORM EDITOR CANVAS ---
            let (rect, response) = ui.allocate_at_least(Vec2::new(ui.available_width(), 200.0), Sense::click_and_drag());
            ui.painter().rect_filled(rect, theme.radius_md, theme.bg_dark);
            ui.painter().rect_stroke(rect, theme.radius_md, theme.border_stroke);

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
                let current_pos = ui.input(|i| i.pointer.latest_pos()).unwrap_or(egui::pos2(0.0, 0.0));
                let x_norm = ((current_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let track_ratio = (start_ratio + x_norm * win_span).clamp(0.0, 1.0);

                if let Some((s_start, _)) = app.editor.editor_selection {
                    app.editor.editor_selection = Some((s_start, track_ratio));
                } else {
                    app.editor.editor_selection = Some((track_ratio, track_ratio));
                }
            }
            if response.clicked() {
                let current_pos = ui.input(|i| i.pointer.latest_pos()).unwrap_or(egui::pos2(0.0, 0.0));
                let x_norm = ((current_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let track_ratio = (start_ratio + x_norm * win_span).clamp(0.0, 1.0);
                app.editor.editor_selection = Some((track_ratio, track_ratio));
            }

            // Draw selection overlay following zoom window
            if let Some((start, end)) = app.editor.editor_selection {
                let s_min = start.min(end);
                let s_max = start.max(end);

                let norm_start = (s_min - start_ratio) / win_span;
                let norm_end = (s_max - start_ratio) / win_span;

                let left = rect.left() + norm_start.clamp(0.0, 1.0) * rect.width();
                let right = rect.left() + norm_end.clamp(0.0, 1.0) * rect.width();

                if right > left {
                    let sel_rect = egui::Rect::from_min_max(egui::pos2(left, rect.top()), egui::pos2(right, rect.bottom()));
                    ui.painter().rect_filled(sel_rect, 0.0, theme.accent.linear_multiply(0.2));
                    ui.painter().rect_stroke(sel_rect, 0.0, egui::Stroke::new(1.0, theme.accent));
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
                        wf.update_from_mip_window(&wgpu.queue, &track.metadata.mip_waveform, start_ratio, end_ratio, rect.width() as u32, color);
                    } else {
                        wf.update_from_band_window(&wgpu.queue, &track.metadata.band_waveform, start_ratio, end_ratio, rect.width() as u32, style, color, [1.0, 1.0, 1.0]);
                    }
                }

                nullherz_ui_hal::render::waveform_renderer::ui_paint_waveform(ui, rect, wf_lock.clone());
            }

            // Draw visual transient chop marks following zoom & scroll
            let total_samples = track.metadata.total_samples;
            if total_samples > 0 {
                let transient_stroke = egui::Stroke::new(1.5_f32, theme.success.linear_multiply(0.8));
                for &t in track.metadata.transients.iter() {
                    let t_ratio = t as f32 / total_samples as f32;
                    let norm_x = (t_ratio - start_ratio) / win_span;
                    if (0.0..=1.0).contains(&norm_x) {
                        let x = rect.left() + norm_x * rect.width();
                        ui.painter().line_segment(
                            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                            transient_stroke,
                        );
                    }
                }
            }

            ui.add_space(theme.space_xs);

            // Waveform Controls Toolbar & Style Selectors
            ui.horizontal(|ui| {
                ui.label(RichText::new("ZOOM").size(theme.type_caption).strong());
                ui.add(egui::Slider::new(&mut app.editor.editor_waveform_zoom, 1.0..=32.0).logarithmic(true).text(""));

                ui.add_space(theme.space_sm);
                ui.label(RichText::new("SCROLL").size(theme.type_caption).strong());
                ui.add_enabled(max_scroll > 0.0, egui::Slider::new(&mut app.editor.editor_waveform_scroll, 0.0..=max_scroll).text(""));

                ui.add_space(theme.space_sm);
                if ui.button(RichText::new(format!("{} RESET VIEW", egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)).size(theme.type_caption)).clicked() {
                    app.editor.editor_waveform_zoom = 1.0;
                    app.editor.editor_waveform_scroll = 0.0;
                }

                ui.add_space(15.0);
                ui.label(RichText::new("STYLE:").size(theme.type_caption).strong().color(theme.accent));
                let curr_style = app.mixer.waveform_styles[0];
                for style in nullherz_ui_hal::render::waveform_renderer::WaveformStyle::all() {
                    let is_sel = curr_style == *style;
                    if ui.selectable_label(is_sel, style.name()).clicked() {
                        app.mixer.waveform_styles[0] = *style;
                    }
                }
            });

            ui.add_space(theme.space_sm);

            // --- 3. SELECTION STATS & SELECTION MANAGEMENT BANNER ---
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

            ui.add_space(theme.space_md);

            // --- 4. ORGANIZED DSP TOOLCARDS ---
            ui.columns(3, |cols| {
                // Card 1: Sample Operations
                cols[0].group(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} SAMPLE OPERATIONS", egui_phosphor::regular::SCISSORS)).strong().size(theme.type_body).color(theme.accent));
                        });
                        ui.add_space(theme.space_xs);

                        let has_selection = app.editor.editor_selection.is_some();
                        ui.add_enabled_ui(has_selection, |ui| {
                            if ui.button(RichText::new(format!("{} CROP SELECTION", egui_phosphor::regular::CROP)).size(theme.type_label)).clicked()
                                && let Some((s, e)) = app.editor.editor_selection {
                                    let (start, end) = if s < e { (s, e) } else { (e, s) };
                                    let total_samples = track.metadata.total_samples as f32;
                                    let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::Crop {
                                        sample_id: track.id,
                                        start_samples: (start * total_samples) as u64,
                                        end_samples: (end * total_samples) as u64,
                                    }));
                                }
                        }).response.on_disabled_hover_text("Select a region on waveform first");

                        ui.add_space(4.0);
                        if ui.button(RichText::new(format!("{} NORMALIZE PEAK (0 dB)", egui_phosphor::regular::LIGHTNING)).size(theme.type_label)).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::Normalize { sample_id: track.id }));
                        }

                        ui.add_space(4.0);
                        if ui.button(RichText::new(format!("{} RE-ANALYZE SOUNDDNA", egui_phosphor::regular::DNA)).size(theme.type_label)).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::ReAnalyze { sample_id: track.id }));
                        }
                    });
                });

                // Card 2: Transient & Chop
                cols[1].group(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} TRANSIENTS & CHOPPING", egui_phosphor::regular::KNIFE)).strong().size(theme.type_body).color(theme.success));
                        });
                        ui.add_space(theme.space_xs);

                        if ui.button(RichText::new(format!("{} CHOP BY TRANSIENTS ({} MARKS)", egui_phosphor::regular::KNIFE, track.metadata.transients.len())).size(theme.type_label)).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::ChopByTransient { sample_id: track.id }));
                        }

                        ui.add_space(4.0);
                        ui.label(RichText::new(format!("Transients Detected: {}", track.metadata.transients.len())).size(theme.type_caption));
                        ui.label(RichText::new(format!("Root Key: {:?}", track.metadata.root_key)).size(theme.type_caption));
                        ui.label(RichText::new(format!("BPM: {:.2}", track.metadata.bpm)).size(theme.type_caption));
                    });
                });

                // Card 3: Time & Pitch DSP
                cols[2].group(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} TIME & PITCH DSP", egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE)).strong().size(theme.type_body).color(theme.warning));
                        });
                        ui.add_space(theme.space_xs);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Stretch Ratio:").size(theme.type_caption));
                            ui.add(egui::Slider::new(&mut app.editor.editor_time_stretch_ratio, 0.5..=2.0).text(""));
                        });

                        ui.add_space(4.0);
                        if ui.button(RichText::new(format!("{} APPLY TIME STRETCH", egui_phosphor::regular::WAVEFORM)).size(theme.type_label)).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::TimeStretch {
                                sample_id: track.id,
                                ratio: app.editor.editor_time_stretch_ratio,
                            }));
                        }
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
