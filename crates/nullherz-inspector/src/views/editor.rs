use egui::{Ui, Vec2, Color32, Stroke, Sense, RichText};
use crate::InspectorApp;
use nullherz_dna::GeneticLibrary;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("Audio Editor");
    ui.add_space(10.0);

    if let Some(track_id) = app.selected_library_track {
        if let Ok(Some(track)) = app.library_db.get_track(track_id) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&track.title).strong().size(18.0));
                ui.label(format!("by {}", track.artist));
            });
            ui.add_space(5.0);
            ui.label(RichText::new(&track.path).size(10.0).color(Color32::GRAY));

            ui.add_space(20.0);

            // Waveform Editor Zone
            let (rect, response) = ui.allocate_at_least(Vec2::new(ui.available_width(), 200.0), Sense::click_and_drag());
            ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(10, 10, 15));
            ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_gray(50)));

            if response.dragged() {
                let current_pos = ui.input(|i| i.pointer.latest_pos()).unwrap_or(egui::pos2(0.0, 0.0));
                let x_norm = ((current_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                if let Some((start, _)) = app.editor_selection {
                    app.editor_selection = Some((start, x_norm));
                } else {
                    app.editor_selection = Some((x_norm, x_norm));
                }
            }
            if response.clicked() {
                let current_pos = ui.input(|i| i.pointer.latest_pos()).unwrap_or(egui::pos2(0.0, 0.0));
                let x_norm = ((current_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                app.editor_selection = Some((x_norm, x_norm));
            }

            if let Some((start, end)) = app.editor_selection {
                let left = rect.left() + start.min(end) * rect.width();
                let right = rect.left() + start.max(end) * rect.width();
                let sel_rect = egui::Rect::from_min_max(egui::pos2(left, rect.top()), egui::pos2(right, rect.bottom()));
                ui.painter().rect_filled(sel_rect, 0.0, Color32::from_white_alpha(30));
            }

            if let Some(wf_lock) = &app.waveform_renderer {
                let mut wf = wf_lock.lock().unwrap();
                let zoom = app.sampler_waveform_zoom;
                let scroll = 0.0;
                let color = [0.0, 1.0, 0.8, 1.0]; // Teal theme

                if let Some(wgpu) = &app.wgpu_renderer {
                    let wgpu = wgpu.lock().unwrap();
                    wf.update_globals(&wgpu.queue, scroll, zoom, color);
                    wf.update_from_mip_waveform(&wgpu.queue, &track.metadata.mip_waveform, zoom, rect.width() as u32);
                }

                nullherz_ui_hal::render::waveform_renderer::ui_paint_waveform(ui, rect, wf_lock.clone());
            }

            ui.add_space(20.0);
            ui.horizontal(|ui| {
                ui.label("ZOOM");
                ui.add(egui::Slider::new(&mut app.sampler_waveform_zoom, 0.1..=10.0).text(""));

                ui.add_space(20.0);
                if ui.button("⟲ RESET").clicked() {
                    app.sampler_waveform_zoom = 1.0;
                }
            });

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("METADATA");
                    ui.group(|ui| {
                        ui.label(format!("BPM: {:.2}", track.metadata.bpm));
                        ui.label(format!("Root Key: {:?}", track.metadata.root_key));
                        ui.label(format!("Transients: {}", track.metadata.transients.len()));
                    });
                });

                ui.add_space(20.0);

                ui.vertical(|ui| {
                    ui.label("ACTIONS");
                    ui.horizontal(|ui| {
                        if ui.button("✂ CROP").clicked() {
                            if let Some((s, e)) = app.editor_selection {
                                let (start, end) = if s < e { (s, e) } else { (e, s) };
                                let total_samples = track.metadata.total_samples as f32;
                                let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::Crop {
                                    sample_id: track.id,
                                    start_samples: (start * total_samples) as u64,
                                    end_samples: (end * total_samples) as u64,
                                }));
                            }
                        }
                        if ui.button("⚡ NORMALIZE").clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::Normalize { sample_id: track.id }));
                        }
                        if ui.button("🧬 RE-ANALYZE DNA").clicked() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::ReAnalyze { sample_id: track.id }));
                        }
                    });
                });
            });

        } else {
            ui.label(RichText::new("Track not found in library.").color(Color32::RED));
            if ui.button("Deselect").clicked() { app.selected_library_track = None; }
        }
    } else {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.label(RichText::new("NO TRACK SELECTED").size(20.0).color(Color32::from_gray(80)));
            ui.label("Select a track from the library to begin editing.");
            if ui.button("OPEN LIBRARY").clicked() {
                app.active_right_tab = Some(crate::RightTab::Library);
            }
        });
    }
}
