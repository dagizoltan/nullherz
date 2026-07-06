use egui::{Ui, Vec2, Color32, Stroke};
use crate::InspectorApp;
use audio_core::Telemetry;

pub fn render_deck_waveform_zone(app: &InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>, deck_color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 60.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(10, 10, 15));

    if let Some(wf_lock) = &app.waveform_renderer {
        if let Some(track_id) = app.now_playing[i] {
            let mut wf = wf_lock.lock().unwrap();
            let zoom = 1.0;
            let scroll = 0.0;
            let color = deck_color.to_array().map(|v| v as f32 / 255.0);

            let track = app.library_db.get_track(track_id).ok().flatten();

            if let Some(ref t) = track {
                if let Some(wgpu) = &app.wgpu_renderer {
                    let wgpu = wgpu.lock().unwrap();
                    wf.update_globals(&wgpu.queue, scroll, zoom, color);
                    wf.update_from_mip_waveform(&wgpu.queue, &t.metadata.mip_waveform, zoom, rect.width() as u32);
                }
            }

            let title = track.as_ref().map(|t| t.title.as_str()).unwrap_or("LOADING...");
            ui.painter().text(rect.left_top() + Vec2::new(5.0, 5.0), egui::Align2::LEFT_TOP, title, egui::FontId::monospace(10.0), deck_color.gamma_multiply(0.8));

            // Render playhead
            let playhead_x = rect.min.x + (telemetry.as_ref().map(|t| (t.beat_position % 4.0) / 4.0).unwrap_or(0.0) as f32 * rect.width());
            ui.painter().line_segment([egui::pos2(playhead_x, rect.min.y), egui::pos2(playhead_x, rect.max.y)], egui::Stroke::new(1.0, Color32::WHITE));
        } else {
            // Enhanced EMPTY DECK visualization
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "EMPTY DECK", egui::FontId::monospace(12.0), Color32::from_gray(60));
            // Render a dashed border for the empty zone
            ui.painter().rect_stroke(rect.shrink(2.0), 2.0, Stroke::new(1.0, Color32::from_gray(30)));
        }
    }
}
