use egui::{Ui, Vec2, Color32, Stroke};
use crate::InspectorApp;
use audio_core::Telemetry;

/// Seconds of audio visible in a deck lane. The playhead sits at the fixed
/// center needle: half the lane is history, half is what's coming.
const NEEDLE_WINDOW_SECS: f32 = 8.0;

/// Deck lane waveform — NEEDLE STYLE for every deck: a zoomed window that
/// scrolls under a fixed center line, the beat-matching view. (The lanes
/// used to show static whole-track overviews with a moving playhead; the
/// full-track position survives as the thin progress bar along the bottom
/// plus the elapsed/total readout.)
pub fn render_deck_waveform_zone(app: &InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>, deck_color: Color32, height: f32) {
    let theme = app.theme;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), egui::Sense::hover());
    ui.painter().rect_filled(rect, theme.radius_sm, theme.bg_inset);

    let track = app.decks.cached_tracks[i].clone();

    let Some(ref t) = track else {
        // Enhanced EMPTY DECK visualization (always shown regardless of GPU/WGPU availability!)
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "EMPTY DECK", egui::FontId::monospace(theme.type_caption), theme.text_disabled);
        // Render a dashed border for the empty zone
        ui.painter().rect_stroke(rect.shrink(2.0), theme.radius_sm, Stroke::new(1.0, theme.border));
        return;
    };

    let sr = 44_100.0f32;
    let total_frames = t.metadata.total_samples.max(1);
    let elapsed_samples = telemetry.as_ref().map(|tel| tel.deck_positions[i]).unwrap_or(0);

    let window_frames = NEEDLE_WINDOW_SECS * sr;
    let center = elapsed_samples as f32;
    let win_start = center as f64 - (window_frames as f64) * 0.5;
    let win_end = center as f64 + (window_frames as f64) * 0.5;
    let start_ratio = (win_start / total_frames as f64) as f32;
    let end_ratio = (win_end / total_frames as f64) as f32;

    if let Some(wf_lock) = &app.deck_waveform_renderers[i] {
        let mut wf = wf_lock.lock();
        let color = deck_color.to_array().map(|v| v as f32 / 255.0);

        if let Some(wgpu) = &app.wgpu_renderer {
            let wgpu = wgpu.lock();
            wf.update_globals(&wgpu.queue, 0.0, 1.0, color);
            if t.metadata.band_waveform.is_empty() {
                // Pre-band library rows: mono silhouette in the deck color.
                wf.update_from_mip_window(&wgpu.queue, &t.metadata.mip_waveform, start_ratio, end_ratio, rect.width() as u32, color);
            } else {
                wf.update_from_band_window(&wgpu.queue, &t.metadata.band_waveform, start_ratio, end_ratio, rect.width() as u32);
            }
        }

        nullherz_ui_hal::render::waveform_renderer::ui_paint_waveform(ui, rect, wf_lock.clone());
    } else {
        // Draw simulated fallback waveform lines when GPU/WGPU is unavailable
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, format!("{} (NO GPU)", t.title), egui::FontId::monospace(theme.type_caption), theme.text_secondary);
        ui.painter().line_segment(
            [egui::pos2(rect.min.x, rect.center().y), egui::pos2(rect.max.x, rect.center().y)],
            egui::Stroke::new(1.0, theme.border)
        );
    }

    let to_x = |frame_pos: f64| -> f32 {
        rect.min.x + ((frame_pos - win_start) / (win_end - win_start)) as f32 * rect.width()
    };

    // Beat grid inside the window: downbeats full-height and brighter.
    if t.metadata.bpm > 20.0 {
        let spb = sr as f64 * 60.0 / t.metadata.bpm as f64;
        let offset = t.metadata.beat_grid_offset as f64;
        let first_beat = (((win_start - offset) / spb).floor().max(0.0)) as u64;
        let mut b = first_beat;
        loop {
            let bpos = offset + b as f64 * spb;
            if bpos > win_end || bpos > total_frames as f64 || b > first_beat + 512 {
                break;
            }
            if bpos >= win_start {
                let x = to_x(bpos);
                let (h, alpha) = if b % 4 == 0 {
                    (rect.height(), 56)
                } else {
                    (rect.height() * 0.4, 30)
                };
                ui.painter().line_segment(
                    [egui::pos2(x, rect.max.y - h), egui::pos2(x, rect.max.y)],
                    egui::Stroke::new(1.0, Color32::from_white_alpha(alpha)),
                );
            }
            b += 1;
        }
    }

    // Hot cue markers inside the window: accent notch + slot number.
    for (ci, cue) in t.metadata.hot_cues.iter().enumerate() {
        if let Some(p) = cue {
            let p = *p as f64;
            if p >= win_start && p <= win_end {
                let x = to_x(p);
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
    }

    // The fixed center needle (cased for contrast on any band color).
    let cx = rect.center().x;
    ui.painter().line_segment(
        [egui::pos2(cx, rect.min.y), egui::pos2(cx, rect.max.y)],
        egui::Stroke::new(4.0, Color32::from_black_alpha(160)),
    );
    ui.painter().line_segment(
        [egui::pos2(cx, rect.min.y), egui::pos2(cx, rect.max.y)],
        egui::Stroke::new(2.0, theme.text_primary),
    );

    // Whole-track position context: a thin progress bar along the bottom
    // edge (the old overview's job, in 3 pixels).
    let progress = (elapsed_samples as f32 / total_frames as f32).clamp(0.0, 1.0);
    let bar_y = rect.max.y - 3.0;
    ui.painter().rect_filled(
        egui::Rect::from_min_max(egui::pos2(rect.min.x, bar_y), egui::pos2(rect.max.x, rect.max.y)),
        0.0,
        Color32::from_black_alpha(120),
    );
    ui.painter().rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(rect.min.x, bar_y),
            egui::pos2(rect.min.x + progress * rect.width(), rect.max.y),
        ),
        0.0,
        deck_color,
    );
    // Cue positions on the progress bar keep whole-track orientation.
    for cue in t.metadata.hot_cues.iter().flatten() {
        let x = rect.min.x + (*cue as f32 / total_frames as f32).clamp(0.0, 1.0) * rect.width();
        ui.painter().vline(x, bar_y..=rect.max.y, Stroke::new(1.0, theme.accent));
    }

    // Elapsed / total time readout, anchored top-left of the zone.
    let fmt = |samples: u64| -> String {
        let s = samples as f64 / sr as f64;
        format!("{}:{:04.1}", (s / 60.0) as u64, s % 60.0)
    };
    ui.painter().text(
        egui::pos2(rect.min.x + 6.0, rect.min.y + 4.0),
        egui::Align2::LEFT_TOP,
        format!("{} / {}", fmt(elapsed_samples), fmt(total_frames)),
        egui::FontId::monospace(theme.type_caption),
        theme.text_primary,
    );
}
