use egui::{Ui, ScrollArea, Color32, Frame, Vec2, Sense, Stroke, RichText, Align2};
use crate::InspectorApp;
use audio_core::Telemetry;
use egui_wgpu::wgpu;
use std::sync::{Arc, Mutex};
use nullherz_dna::GeneticLibrary;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        ui.heading("Precision Media Player");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("📂 LOAD FOLDER").clicked() {
                // Folder monitor logic would go here
            }
        });
    });
    ui.add_space(10.0);

    // Main Player Display (Waveform + Info)
    Frame::none().fill(Color32::from_rgb(15, 15, 18)).rounding(6.0).inner_margin(16.0).show(ui, |ui| {
        ui.vertical(|ui| {
            // Waveform Display
            let (rect, _response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 220.0), Sense::hover());

            if let (Some(wgpu_mtx), Some(wf)) = (&app.wgpu_renderer, &app.waveform_renderer) {
                let _wgpu = wgpu_mtx.lock().unwrap();

                // Use a dedicated 'Player' deck or focused deck for visualization
                let deck_idx = app.focused_deck;
                if let Some(track_id) = app.now_playing[deck_idx] {
                    if let Ok(Some(track)) = app.library_db.get_track(track_id) {
                        wf.update_from_mip_waveform(&_wgpu.queue, &track.metadata.mip_waveform, app.sampler_waveform_zoom, rect.width() as u32);

                        // Metadata Overlay
                        ui.painter().text(rect.left_top() + Vec2::new(10.0, 10.0), Align2::LEFT_TOP, format!("{} - {}", track.artist, track.title), egui::FontId::proportional(18.0), Color32::WHITE);
                        ui.painter().text(rect.left_top() + Vec2::new(10.0, 32.0), Align2::LEFT_TOP, format!("BPM: {:.1} | KEY: {}", track.metadata.bpm, track.metadata.root_key.unwrap_or(0.0)), egui::FontId::proportional(12.0), Color32::from_gray(180));
                    }
                }

                if let Some(t) = telemetry {
                    let scroll = (t.beat_position as f32 % 4.0) / 4.0 * 2.0;
                    wf.update_globals(&_wgpu.queue, scroll, app.sampler_waveform_zoom, [0.1, 0.4, 1.0, 1.0]);
                }

                struct WaveformCallback {
                    renderer: Arc<nullherz_ui_hal::render::waveform_renderer::WaveformRenderer>,
                }
                impl egui_wgpu::CallbackTrait for WaveformCallback {
                    fn paint<'a>(&'a self, _info: egui::PaintCallbackInfo, render_pass: &mut wgpu::RenderPass<'a>, _resources: &egui_wgpu::CallbackResources) {
                        self.renderer.render(render_pass);
                    }
                }
                let callback = egui_wgpu::Callback::new_paint_callback(rect, WaveformCallback { renderer: wf.clone() });
                ui.painter().add(callback);
            }

            if let Some(t) = telemetry {
                let playhead_x = rect.left() + (t.beat_position as f32 % 4.0) / 4.0 * rect.width();
                ui.painter().line_segment([egui::pos2(playhead_x, rect.top()), egui::pos2(playhead_x, rect.bottom())], Stroke::new(2.0, Color32::from_rgb(0, 150, 255)));
            }

            ui.add_space(10.0);

            // Transport Controls
            ui.horizontal_centered(|ui| {
                ui.add_space(ui.available_width() / 4.0);
                if ui.add(egui::Button::new(RichText::new("⏮").size(24.0)).min_size(Vec2::splat(40.0))).clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpByBeats { node_idx: 100, beats: -4.0 }));
                }
                ui.add_space(10.0);

                let play_btn = if app.player_is_playing {
                    egui::Button::new(RichText::new("⏸").size(32.0)).fill(Color32::from_rgb(0, 100, 200))
                } else {
                    egui::Button::new(RichText::new("▶").size(32.0))
                };

                if ui.add(play_btn.min_size(Vec2::splat(60.0))).clicked() {
                    app.player_is_playing = !app.player_is_playing;
                    if app.player_is_playing {
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play));
                    } else {
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop));
                    }
                }
                ui.add_space(10.0);
                if ui.add(egui::Button::new(RichText::new("⏭").size(24.0)).min_size(Vec2::splat(40.0))).clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::JumpByBeats { node_idx: 100, beats: 4.0 }));
                }
                ui.add_space(20.0);
                ui.label(RichText::new("GAIN").small());
                ui.add(egui::Slider::new(&mut app.master_gain, 0.0..=1.5).show_value(false));
            });
        });
    });

    ui.add_space(20.0);
    ui.separator();
    ui.add_space(10.0);

    // Library / Playlists
    ui.horizontal(|ui| {
        ui.heading("Library Browser");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.text_edit_singleline(&mut app.search_query);
            ui.label("🔍");
        });
    });
    ui.add_space(10.0);

    ScrollArea::vertical().show(ui, |ui| {
        if app.cached_library.is_empty() && app.library_needs_refresh {
            if let Ok(tracks) = app.library_db.list_tracks() {
                app.cached_library = tracks;
                app.library_needs_refresh = false;
            }
        }

        egui::Grid::new("library_grid").num_columns(5).spacing([20.0, 8.0]).striped(true).show(ui, |ui| {
            ui.label(RichText::new("TITLE").strong());
            ui.label(RichText::new("ARTIST").strong());
            ui.label(RichText::new("BPM").strong());
            ui.label(RichText::new("KEY").strong());
            ui.label("");
            ui.end_row();

            let query = app.search_query.to_lowercase();
            for track in &app.cached_library {
                if !query.is_empty() && !track.title.to_lowercase().contains(&query) && !track.artist.to_lowercase().contains(&query) {
                    continue;
                }

                ui.label(&track.title);
                ui.label(&track.artist);
                ui.label(format!("{:.1}", track.metadata.bpm));
                ui.label(format!("{}", track.metadata.root_key.unwrap_or(0.0)));

                if ui.button("LOAD").clicked() {
                    let deck_char = (b'A' + app.focused_deck as u8) as char;
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::LoadTrackToDeck {
                        deck_id: deck_char,
                        sample_id: track.id,
                    }));
                    app.now_playing[app.focused_deck] = Some(track.id);
                }
                ui.end_row();
            }
        });
    });
}
