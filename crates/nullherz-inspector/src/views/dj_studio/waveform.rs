use egui::{Ui, Vec2, Color32, Stroke};
use crate::InspectorApp;
use audio_core::Telemetry;

/// Seconds of audio visible in a deck lane. The playhead sits at the fixed
/// center needle: half the lane is history, half is what's coming.
const NEEDLE_WINDOW_SECS: f32 = 8.0;

/// Seconds of audio the playhead moves per unit of wheel scroll.
///
/// A mouse notch is ~50 raw units, so a notch moves ~0.25 s — a nudge you can
/// hear without losing your place. A trackpad emits many small deltas and
/// scrubs smoothly at the same constant.
///
/// Expressed in TIME, not beats. The previous version sent `JumpByBeats`, which
/// the sampler ignores unless it has a transport AND `metadata.bpm > 0.0`, so
/// scrubbing silently did nothing on any track whose tempo had not been
/// detected — and gave a different distance per notch depending on the track's
/// tempo, which is not what dragging a needle should feel like.
const SCRUB_SECONDS_PER_SCROLL_UNIT: f32 = 0.005;

/// Hold SHIFT for a fine scrub — a tenth of the distance, for setting a cue
/// point exactly.
const SCRUB_FINE_FACTOR: f32 = 0.1;

/// Ceiling on scratch speed, in multiples of normal playback.
///
/// A fast flick across a short lane can compute an enormous rate from one
/// frame's drag delta, and the voice advances `rate` frames per output sample —
/// so an unclamped spike would tear through the buffer in a single block and
/// deactivate the voice mid-gesture. 8x forward or back is well past anything
/// a hand does on a platter.
const MAX_SCRATCH_RATE: f32 = 8.0;

/// Deck lane waveform — NEEDLE STYLE for every deck: a zoomed window that
/// scrolls under a fixed center line, the beat-matching view. (The lanes
/// used to show static whole-track overviews with a moving playhead; the
/// full-track position survives as the thin progress bar along the bottom
/// plus the elapsed/total readout.)
pub fn render_deck_waveform_zone(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>, deck_color: Color32, height: f32) {
    let theme = app.theme;
    // The waveform owns its own gestures: drag to scratch, click to focus.
    // (`render_waveform_lane` used to `interact` over the whole lane, which won
    // every pointer event here and made scroll-to-scrub dead code; it now covers
    // only the header strip.)
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        egui::Sense::click_and_drag(),
    );
    ui.painter().rect_filled(rect, theme.radius_sm, theme.bg_inset);

    let track = app.decks.cached_tracks[i].clone();

    let Some(ref t) = track else {
        // Enhanced EMPTY DECK visualization (always shown regardless of GPU/WGPU availability!)
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "EMPTY DECK", egui::FontId::monospace(theme.type_caption), theme.text_disabled);
        // Render a dashed border for the empty zone
        ui.painter().rect_stroke(rect.shrink(2.0), theme.radius_sm, Stroke::new(1.0, theme.border));
        return;
    };

    // Scroll-to-scrub: hovering a loaded deck lane and rolling the wheel seeks
    // the deck's playhead. JumpByBeats moves the sampler voice whether it is
    // playing or paused, so the needle follows the wheel. Consume the scroll
    // delta so the surrounding mixer ScrollArea does not pan at the same time.
    // Pointer containment rather than `response.hovered()`.
    //
    // Robustness, NOT a fix: `response.hovered()` was suspected of being
    // starved by the lane-wide `ui.interact()` in `render_waveform_lane`, but
    // the headless UI harness renders this view with both the old gate and the
    // old lane-wide interaction and the scroll still lands. That diagnosis was
    // wrong; scrubbing was broken by the two engine-side causes described on
    // `PerformanceCommand::NudgePosition`.
    //
    // Kept because it is the stronger formulation regardless — it asks where
    // the pointer IS rather than which widget won the claim to it, so adding an
    // overlay here later cannot silently kill scrubbing.
    if ui.rect_contains_pointer(rect) {
        let (scroll_y, fine) = ui.input(|inp| (inp.raw_scroll_delta.y, inp.modifiers.shift));
        if scroll_y.abs() > f32::EPSILON {
            let node_name = match i {
                0 => "deck_a_sampler",
                1 => "deck_b_sampler",
                2 => "deck_c_sampler",
                3 => "deck_d_sampler",
                _ => "",
            };
            if let Some(node_idx) = app.get_node_id(node_name) {
                // Wheel-up (positive delta) moves forward through the track.
                //
                // Frames are the TRACK's own frames: `play_head` indexes the
                // decoded source buffer, so a 44.1 kHz file scrubs by its own
                // sample rate regardless of what the device negotiated.
                let src_sr = t.metadata.sample_rate.max(1) as f32;
                let scale = if fine { SCRUB_FINE_FACTOR } else { 1.0 };
                let frames = (scroll_y * SCRUB_SECONDS_PER_SCROLL_UNIT * scale * src_sr) as i64;
                if frames != 0 {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                        nullherz_traits::PerformanceCommand::NudgePosition { node_idx, frames },
                    ));
                }
                ui.input_mut(|inp| {
                    inp.raw_scroll_delta = Vec2::ZERO;
                    inp.smooth_scroll_delta = Vec2::ZERO;
                });
            }
        }
    }

    // --- scratch ------------------------------------------------------------
    //
    // Drag the waveform and the deck plays at the speed of your hand, backwards
    // included. Rate rather than position is what makes it sound like a record:
    // moving the playhead in jumps just chops the audio, while handing the voice
    // the gesture's rate reproduces the pitch bend.
    //
    // Direction follows the waveform, not the track: the lane scrolls right to
    // left as it plays, so dragging LEFT pulls the track forward.
    if response.dragged() || response.drag_stopped() {
        let node_name = match i {
            0 => "deck_a_sampler",
            1 => "deck_b_sampler",
            2 => "deck_c_sampler",
            3 => "deck_d_sampler",
            _ => "",
        };
        if let Some(node_idx) = app.get_node_id(node_name) {
            if response.drag_stopped() {
                // Let go: the deck goes back to whatever it was doing.
                let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                    nullherz_traits::PerformanceCommand::SetScratch { node_idx, active: false, rate: 0.0 },
                ));
            } else {
                let src_sr = t.metadata.sample_rate.max(1) as f32;
                let dt = ui.input(|inp| inp.stable_dt).max(1.0e-4);
                // Pixels -> source frames, via the lane's own time window.
                let frames_per_px = (NEEDLE_WINDOW_SECS * src_sr) / rect.width().max(1.0);
                let frames_moved = -response.drag_delta().x * frames_per_px;
                // Normal playback covers src_sr frames per second, so this is
                // the gesture expressed as a playback-rate multiplier.
                let rate = (frames_moved / (src_sr * dt)).clamp(-MAX_SCRATCH_RATE, MAX_SCRATCH_RATE);
                let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                    nullherz_traits::PerformanceCommand::SetScratch { node_idx, active: true, rate },
                ));
            }
        }
    }
    if response.clicked() {
        app.decks.focused_deck = i;
    }

    // The track's OWN rate, not the device's. Every quantity below —
    // `total_frames`, `deck_positions`, the beat grid, hot cues — is measured in
    // SOURCE frames, so converting them to seconds or to a needle window must
    // divide by the rate those frames were recorded at. A hardcoded 44.1 kHz put
    // the beat grid and the elapsed/total readout 8.8% out on 48 kHz material.
    let sr = (t.metadata.sample_rate.max(1)) as f32;
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

                // Elegant beat counters (1, 2, 3, 4) indicating bar/beat context to the DJ
                let beat_num = (b % 4) + 1;
                ui.painter().text(
                    egui::pos2(x + 3.0, rect.max.y - h + 2.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}", beat_num),
                    egui::FontId::monospace(8.0),
                    if b % 4 == 0 { theme.accent } else { theme.text_disabled },
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

    // Glowing visual pulse on the active playhead needle matching the current beat/tempo
    let is_playing = app.decks.deck_playing[i];
    if is_playing {
        let beat_pos = telemetry.as_ref().map(|tel| tel.beat_position).unwrap_or(0.0);
        let pulse = (1.0 - (beat_pos % 1.0) as f32).powf(3.0);
        let glow_color = deck_color.linear_multiply(pulse * 0.8);
        ui.painter().line_segment(
            [egui::pos2(cx, rect.min.y), egui::pos2(cx, rect.max.y)],
            egui::Stroke::new(6.0 + pulse * 8.0, glow_color),
        );
    }

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
