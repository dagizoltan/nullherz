use egui::{Ui, Vec2, Sense, RichText, Frame, Margin};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;

    if let Some(track_id) = app.library.selected_library_track {
        if let Some(track) = app.get_cached_track(track_id) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&track.title).strong().size(theme.type_heading));
                ui.label(RichText::new(format!("by {}", track.artist)).size(theme.type_body));
            });
            ui.add_space(theme.space_xs);
            ui.label(RichText::new(&track.path).size(theme.type_caption).color(theme.text_secondary));

            ui.add_space(theme.space_md);

            // Compute visible window derived from editor zoom and scroll
            let zoom = app.editor.editor_waveform_zoom.clamp(1.0, 32.0);
            let win_span = 1.0 / zoom;
            let max_scroll = (1.0 - win_span).max(0.0);
            let scroll = app.editor.editor_waveform_scroll.clamp(0.0, max_scroll);
            let start_ratio = scroll;
            let end_ratio = (scroll + win_span).min(1.0);

            // Waveform Editor Zone
            let (rect, response) = ui.allocate_at_least(Vec2::new(ui.available_width(), 200.0), Sense::click_and_drag());
            ui.painter().rect_filled(rect, theme.radius_md, theme.bg_dark);
            ui.painter().rect_stroke(rect, theme.radius_md, theme.border_stroke);

            // Mouse wheel zoom support
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

            ui.add_space(theme.space_md);
            ui.horizontal(|ui| {
                ui.label(RichText::new("ZOOM").size(theme.type_body));
                ui.add(egui::Slider::new(&mut app.editor.editor_waveform_zoom, 1.0..=32.0).logarithmic(true).text(""));

                ui.add_space(theme.space_md);
                ui.label(RichText::new("SCROLL").size(theme.type_body));
                ui.add_enabled(max_scroll > 0.0, egui::Slider::new(&mut app.editor.editor_waveform_scroll, 0.0..=max_scroll).text(""));

                ui.add_space(theme.space_md);
                if ui.button(RichText::new(format!("{} RESET VIEW", egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)).size(theme.type_label)).clicked() {
                    app.editor.editor_waveform_zoom = 1.0;
                    app.editor.editor_waveform_scroll = 0.0;
                }
            });

            ui.add_space(theme.space_md);
            ui.separator();
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("METADATA").size(theme.type_body).strong());
                    Frame::none()
                        .fill(theme.bg_inset)
                        .rounding(theme.radius_md)
                        .stroke(theme.border_stroke)
                        .inner_margin(Margin::same(theme.space_sm))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(format!("BPM: {:.2}", track.metadata.bpm)).size(theme.type_caption));
                                ui.label(RichText::new(format!("Root Key: {:?}", track.metadata.root_key)).size(theme.type_caption));
                                ui.label(RichText::new(format!("Transients: {}", track.metadata.transients.len())).size(theme.type_caption));
                            });
                        });
                });

                ui.add_space(theme.space_md);

                ui.vertical(|ui| {
                    ui.label(RichText::new("ACTIONS").size(theme.type_body).strong());
                    ui.horizontal(|ui| {
                        let has_selection = app.editor.editor_selection.is_some();
                        ui.add_enabled_ui(has_selection, |ui| {
                            let btn = ui.button(RichText::new(format!("{} CROP", egui_phosphor::regular::SCISSORS)).size(theme.type_label));
                            if btn.clicked()
                                && let Some((s, e)) = app.editor.editor_selection {
                                    let (start, end) = if s < e { (s, e) } else { (e, s) };
                                    let total_samples = track.metadata.total_samples as f32;
                                    let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::Crop {
                                        sample_id: track.id,
                                        start_samples: (start * total_samples) as u64,
                                        end_samples: (end * total_samples) as u64,
                                    }));
                                }
                        }).response.on_disabled_hover_text("Drag on the waveform to select a range first");

                        if ui.button(RichText::new(format!("{} NORMALIZE", egui_phosphor::regular::LIGHTNING)).size(theme.type_label)).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::Normalize { sample_id: track.id }));
                        }
                        if ui.button(RichText::new(format!("{} RE-ANALYZE DNA", egui_phosphor::regular::DNA)).size(theme.type_label)).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::ReAnalyze { sample_id: track.id }));
                        }
                    });

                    ui.add_space(theme.space_sm);

                    ui.horizontal(|ui| {
                        // Transient Chopping Action
                        if ui.button(RichText::new(format!("{} CHOP BY TRANSIENT", egui_phosphor::regular::KNIFE)).size(theme.type_label)).clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::ChopByTransient { sample_id: track.id }));
                        }

                        ui.add_space(theme.space_md);

                        // Time Stretching Actions
                        ui.label(RichText::new("Ratio:").size(theme.type_body));
                        ui.add(egui::Slider::new(&mut app.editor.editor_time_stretch_ratio, 0.5..=2.0).text(""));

                        if ui.button(RichText::new(format!("{} TIME STRETCH", egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE)).size(theme.type_label)).clicked() {
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
            if ui.button(RichText::new("OPEN LIBRARY").size(theme.type_label)).clicked() {
                app.active_right_tab = Some(crate::RightTab::Library);
            }
        });
    }
}
