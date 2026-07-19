use egui::{Ui, Vec2, Color32, Stroke};
use crate::InspectorApp;
use audio_core::Telemetry;

pub fn render_deck_waveform_zone(app: &InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>, deck_color: Color32, height: f32) {
    let theme = app.theme;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), egui::Sense::hover());
    ui.painter().rect_filled(rect, theme.radius_sm, theme.bg_inset);

    let track = app.decks.cached_tracks[i].clone();

    if let Some(ref t) = track {
        if let Some(wf_lock) = &app.deck_waveform_renderers[i] {
            let mut wf = wf_lock.lock();
            let zoom = 1.0;
            let scroll = 0.0;
            let color = deck_color.to_array().map(|v| v as f32 / 255.0);

            if let Some(wgpu) = &app.wgpu_renderer {
                let wgpu = wgpu.lock();
                wf.update_globals(&wgpu.queue, scroll, zoom, color);
                wf.update_from_mip_waveform(&wgpu.queue, &t.metadata.mip_waveform, zoom, rect.width() as u32);
            }

            nullherz_ui_hal::render::waveform_renderer::ui_paint_waveform(ui, rect, wf_lock.clone());
        } else {
            // Draw simulated fallback waveform lines when GPU/WGPU is unavailable
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, format!("{} (NO GPU)", t.title), egui::FontId::monospace(theme.type_caption), theme.text_secondary);
            // Draw a center line
            ui.painter().line_segment(
                [egui::pos2(rect.min.x, rect.center().y), egui::pos2(rect.max.x, rect.center().y)],
                egui::Stroke::new(1.0, theme.border)
            );
        }

        // Render playhead using actual per-deck playback position.
        // High-contrast (dark casing + light core) — the old single line in
        // deck_color was invisible on top of a waveform of the same color.
        let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions[i]).unwrap_or(0);
        let total_samples = track.as_ref().map(|t| t.metadata.total_samples).unwrap_or(0).max(1);
        let playhead_ratio = elapsed_samples as f32 / total_samples as f32;
        let playhead_x = rect.min.x + (playhead_ratio.clamp(0.0, 1.0) * rect.width());

        ui.painter().line_segment(
            [egui::pos2(playhead_x, rect.min.y), egui::pos2(playhead_x, rect.max.y)],
            egui::Stroke::new(4.0, Color32::from_black_alpha(160))
        );
        ui.painter().line_segment(
            [egui::pos2(playhead_x, rect.min.y), egui::pos2(playhead_x, rect.max.y)],
            egui::Stroke::new(2.0, theme.text_primary)
        );

        // Elapsed / total time readout, anchored top-left of the zone.
        let sr = 44_100.0f64;
        let fmt = |samples: u64| -> String {
            let s = samples as f64 / sr;
            format!("{}:{:04.1}", (s / 60.0) as u64, s % 60.0)
        };
        ui.painter().text(
            egui::pos2(rect.min.x + 6.0, rect.min.y + 4.0),
            egui::Align2::LEFT_TOP,
            format!("{} / {}", fmt(elapsed_samples), fmt(total_samples)),
            egui::FontId::monospace(theme.type_caption),
            theme.text_primary,
        );
    } else {
        // Enhanced EMPTY DECK visualization (always shown regardless of GPU/WGPU availability!)
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "EMPTY DECK", egui::FontId::monospace(theme.type_caption), theme.text_disabled);
        // Render a dashed border for the empty zone
        ui.painter().rect_stroke(rect.shrink(2.0), theme.radius_sm, Stroke::new(1.0, theme.border));
    }
}
