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
                if t.metadata.band_waveform.is_empty() {
                    // Pre-band library rows: mono silhouette in the deck color.
                    wf.update_from_mip_waveform(&wgpu.queue, &t.metadata.mip_waveform, zoom, rect.width() as u32, color);
                } else {
                    wf.update_from_band_waveform(&wgpu.queue, &t.metadata.band_waveform, zoom, rect.width() as u32);
                }
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

        // Beat grid from the analyzed BPM: a tick per beat, full-height and
        // brighter on the downbeat (every 4th). Drawn before the playhead so
        // the needle stays on top.
        let total_frames = t.metadata.total_samples.max(1);
        if t.metadata.bpm > 20.0 {
            let spb = 44_100.0f64 * 60.0 / t.metadata.bpm as f64;
            let total = total_frames as f64;
            let offset = t.metadata.beat_grid_offset as f64;
            let mut b: u64 = 0;
            loop {
                let pos = offset + b as f64 * spb;
                if pos >= total || b > 100_000 {
                    break;
                }
                let x = rect.min.x + (pos / total) as f32 * rect.width();
                let (h, alpha) = if b % 4 == 0 {
                    (rect.height(), 42)
                } else {
                    (rect.height() * 0.35, 22)
                };
                ui.painter().line_segment(
                    [egui::pos2(x, rect.max.y - h), egui::pos2(x, rect.max.y)],
                    egui::Stroke::new(1.0, Color32::from_white_alpha(alpha)),
                );
                b += 1;
            }
        }

        // Hot cue markers: accent notch + slot number at the top edge.
        for (ci, cue) in t.metadata.hot_cues.iter().enumerate() {
            if let Some(pos) = cue {
                let ratio = (*pos as f32 / total_frames as f32).clamp(0.0, 1.0);
                let x = rect.min.x + ratio * rect.width();
                ui.painter().line_segment(
                    [egui::pos2(x, rect.min.y), egui::pos2(x, rect.min.y + 10.0)],
                    egui::Stroke::new(2.0, theme.accent),
                );
                ui.painter().text(
                    egui::pos2(x + 3.0, rect.min.y + 1.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}", ci + 1),
                    egui::FontId::monospace(9.0),
                    theme.accent,
                );
            }
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
