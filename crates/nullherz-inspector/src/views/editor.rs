use egui::{Ui, Vec2, Sense, RichText, Frame, Margin};
use crate::InspectorApp;
use nullherz_dna::GeneticLibrary;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.heading(RichText::new("Audio Editor").size(theme.type_heading));
    ui.add_space(theme.space_sm);

    if let Some(track_id) = app.library.selected_library_track {
        if let Ok(Some(track)) = app.library_db.get_track(track_id) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&track.title).strong().size(theme.type_heading));
                ui.label(RichText::new(format!("by {}", track.artist)).size(theme.type_body));
            });
            ui.add_space(theme.space_xs);
            ui.label(RichText::new(&track.path).size(theme.type_caption).color(theme.text_secondary));

            ui.add_space(theme.space_md);

            // Waveform Editor Zone
            let (rect, response) = ui.allocate_at_least(Vec2::new(ui.available_width(), 200.0), Sense::click_and_drag());
            ui.painter().rect_filled(rect, theme.radius_md, theme.bg_dark);
            ui.painter().rect_stroke(rect, theme.radius_md, theme.border_stroke);

            if response.dragged() {
                let current_pos = ui.input(|i| i.pointer.latest_pos()).unwrap_or(egui::pos2(0.0, 0.0));
                let x_norm = ((current_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                if let Some((start, _)) = app.editor.editor_selection {
                    app.editor.editor_selection = Some((start, x_norm));
                } else {
                    app.editor.editor_selection = Some((x_norm, x_norm));
                }
            }
            if response.clicked() {
                let current_pos = ui.input(|i| i.pointer.latest_pos()).unwrap_or(egui::pos2(0.0, 0.0));
                let x_norm = ((current_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                app.editor.editor_selection = Some((x_norm, x_norm));
            }

            if let Some((start, end)) = app.editor.editor_selection {
                let left = rect.left() + start.min(end) * rect.width();
                let right = rect.left() + start.max(end) * rect.width();
                let sel_rect = egui::Rect::from_min_max(egui::pos2(left, rect.top()), egui::pos2(right, rect.bottom()));
                ui.painter().rect_filled(sel_rect, 0.0, theme.accent.linear_multiply(0.2));
            }

            if let Some(wf_lock) = &app.waveform_renderer {
                let mut wf = wf_lock.lock();
                let zoom = app.sampler.sampler_waveform_zoom;
                let scroll = 0.0;
                let color = theme.accent.to_array().map(|v| v as f32 / 255.0);

                if let Some(wgpu) = &app.wgpu_renderer {
                    let wgpu = wgpu.lock();
                    wf.update_globals(&wgpu.queue, scroll, zoom, color);
                    wf.update_from_mip_waveform(&wgpu.queue, &track.metadata.mip_waveform, zoom, rect.width() as u32, app.theme.accent.to_array().map(|v| v as f32 / 255.0));
                }

                nullherz_ui_hal::render::waveform_renderer::ui_paint_waveform(ui, rect, wf_lock.clone());
            }

            // Draw visual transient markers
            let total_samples = track.metadata.total_samples;
            if total_samples > 0 {
                let transient_stroke = egui::Stroke::new(1.5, theme.success.linear_multiply(0.8));
                for &t in track.metadata.transients.iter() {
                    let ratio = t as f32 / total_samples as f32;
                    let x = rect.left() + ratio * rect.width();
                    ui.painter().line_segment(
                        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                        transient_stroke,
                    );
                }
            }

            ui.add_space(theme.space_md);
            ui.horizontal(|ui| {
                ui.label(RichText::new("ZOOM").size(theme.type_body));
                ui.add(egui::Slider::new(&mut app.sampler.sampler_waveform_zoom, 0.1..=10.0).text(""));

                ui.add_space(theme.space_md);
                if ui.button(RichText::new(format!("{} RESET", egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)).size(theme.type_label)).clicked() {
                    app.sampler.sampler_waveform_zoom = 1.0;
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
