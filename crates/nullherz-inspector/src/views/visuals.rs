//! Visual Mixer View and Neural Visual Surface rendering.
//! Supports multi-channel visual generators, interactive surface controls,
//! and visual post-processing insert rack chaining on PixelFeedbackEngine framebuffers.

use eframe::egui;
use crate::{state, InspectorApp, Telemetry};

/// Apply a chain of visual post-processing insert sidecars onto the PixelFeedbackEngine framebuffer.
pub fn apply_visual_insert_chain(
    engine: &mut state::PixelFeedbackEngine,
    visual_inserts: &[String],
    color_shift: f32,
    time: f32,
) {
    let w = engine.width;
    let h = engine.height;
    if w == 0 || h == 0 {
        return;
    }

    for insert_id in visual_inserts {
        match insert_id.as_str() {
            "phase-goniometer-2d" => {
                // 2D Phase Goniometer: Draw stereo vector phase trajectory onto framebuffer
                let center_x = w as f32 * 0.5;
                let center_y = h as f32 * 0.5;
                let radius = (w.min(h) as f32) * 0.35;
                let points = 32;
                for i in 0..points {
                    let phase = (i as f32 / points as f32) * std::f32::consts::TAU;
                    let l = (phase + time * 3.0).sin();
                    let r = (phase * 1.5 + time * 2.0).cos();
                    let x = center_x + (l - r) * radius * 0.5;
                    let y = center_y - (l + r) * radius * 0.5;
                    let px = x.clamp(0.0, (w - 1) as f32) as usize;
                    let py = y.clamp(0.0, (h - 1) as f32) as usize;
                    let idx = py * w + px;
                    let current = engine.back_buffer[idx];
                    engine.back_buffer[idx] = [
                        current[0].saturating_add(40),
                        current[1].saturating_add(220),
                        current[2].saturating_add(180),
                        255,
                    ];
                }
            }
            "neural-saturation" | "neural-saturator-v" => {
                // Non-linear Padé SIMD color saturation & contrast boost
                let sat_factor = 1.4 + color_shift * 0.6;
                for pixel in engine.back_buffer.iter_mut() {
                    let r = pixel[0] as f32 / 255.0;
                    let g = pixel[1] as f32 / 255.0;
                    let b = pixel[2] as f32 / 255.0;
                    let gray = 0.299 * r + 0.587 * g + 0.114 * b;
                    let r_sat = (gray + (r - gray) * sat_factor).clamp(0.0, 1.0);
                    let g_sat = (gray + (g - gray) * sat_factor).clamp(0.0, 1.0);
                    let b_sat = (gray + (b - gray) * sat_factor).clamp(0.0, 1.0);
                    pixel[0] = (r_sat * 255.0) as u8;
                    pixel[1] = (g_sat * 255.0) as u8;
                    pixel[2] = (b_sat * 255.0) as u8;
                }
            }
            "bloom-filter" => {
                // Anamorphic Bloom Filter: High-pass thresholding & glow diffusion using zero-allocation front_buffer as scratch
                let threshold = 140u8;
                for i in 0..(w * h).min(engine.front_buffer.len()) {
                    let pix = engine.back_buffer[i];
                    let brightness = (pix[0] as u32 + pix[1] as u32 + pix[2] as u32) / 3;
                    engine.front_buffer[i] = if brightness > threshold as u32 {
                        [
                            (pix[0] as f32 * 0.6) as u8,
                            (pix[1] as f32 * 0.6) as u8,
                            (pix[2] as f32 * 0.6) as u8,
                            255,
                        ]
                    } else {
                        [0, 0, 0, 255]
                    };
                }
                // Additive 1D horizontal blur pass for anamorphic streak
                for y in 0..h {
                    for x in 1..(w - 1) {
                        let idx = y * w + x;
                        let left = engine.front_buffer[idx - 1];
                        let right = engine.front_buffer[idx + 1];
                        let curr = &mut engine.back_buffer[idx];
                        curr[0] = curr[0].saturating_add((left[0] as u16 / 4 + right[0] as u16 / 4) as u8);
                        curr[1] = curr[1].saturating_add((left[1] as u16 / 4 + right[1] as u16 / 4) as u8);
                        curr[2] = curr[2].saturating_add((left[2] as u16 / 4 + right[2] as u16 / 4) as u8);
                    }
                }
            }
            "fft-spectrum-mesh" => {
                // FFT Spectrum Mesh: Render bottom frequency bar overlay
                let num_bars = 16.min(w);
                let bar_width = w / num_bars;
                for b in 0..num_bars {
                    let height = (((b as f32 * 0.5 + time * 4.0).sin() * 0.5 + 0.5) * (h as f32 * 0.3)) as usize;
                    let x_start = b * bar_width;
                    let x_end = ((b + 1) * bar_width).min(w);
                    for y in (h.saturating_sub(height))..h {
                        for x in x_start..x_end {
                            let idx = y * w + x;
                            let current = engine.back_buffer[idx];
                            engine.back_buffer[idx] = [
                                current[0].saturating_add(30),
                                current[1].saturating_add(80),
                                current[2].saturating_add(200),
                                255,
                            ];
                        }
                    }
                }
            }
            "reaction-diffusion-nn" => {
                // Reaction Diffusion Neural Network: Discrete Laplacian feedback step
                for y in 1..(h - 1) {
                    for x in 1..(w - 1) {
                        let idx = y * w + x;
                        let neighbor_sum = engine.back_buffer[(y - 1) * w + x][0] as i32
                            + engine.back_buffer[(y + 1) * w + x][0] as i32
                            + engine.back_buffer[y * w + (x - 1)][0] as i32
                            + engine.back_buffer[y * w + (x + 1)][0] as i32;
                        let laplacian = neighbor_sum / 4 - engine.back_buffer[idx][0] as i32;
                        let diff = (laplacian as f32 * 0.15) as i32;
                        let current = engine.back_buffer[idx];
                        engine.back_buffer[idx] = [
                            (current[0] as i32 + diff).clamp(0, 255) as u8,
                            (current[1] as i32 + diff / 2).clamp(0, 255) as u8,
                            (current[2] as i32 - diff / 2).clamp(0, 255) as u8,
                            255,
                        ];
                    }
                }
            }
            _ => {
                // Generic visual post-processing tint shift
                let shift = (time * 2.0).sin() * 20.0;
                for pixel in engine.back_buffer.iter_mut() {
                    pixel[0] = (pixel[0] as i16 + shift as i16).clamp(0, 255) as u8;
                }
            }
        }
    }

    // Sync back_buffer updates to front_buffer for continuous feedback
    engine.front_buffer.copy_from_slice(&engine.back_buffer);
}

/// Blend source RGBA pixel onto destination RGBA pixel using VisualBlendMode & opacity
pub fn blend_pixel(src: [u8; 4], dst: [u8; 4], mode: state::VisualBlendMode, opacity: f32) -> [u8; 4] {
    let op = opacity.clamp(0.0, 1.0);
    if op <= 0.001 {
        return dst;
    }

    let src_alpha = (src[3] as f32 / 255.0) * op;
    let sr = src[0] as f32 / 255.0;
    let sg = src[1] as f32 / 255.0;
    let sb = src[2] as f32 / 255.0;

    let dr = dst[0] as f32 / 255.0;
    let dg = dst[1] as f32 / 255.0;
    let db = dst[2] as f32 / 255.0;

    let (out_r, out_g, out_b) = match mode {
        state::VisualBlendMode::Normal => (
            sr * src_alpha + dr * (1.0 - src_alpha),
            sg * src_alpha + dg * (1.0 - src_alpha),
            sb * src_alpha + db * (1.0 - src_alpha),
        ),
        state::VisualBlendMode::Additive => (
            (dr + sr * op).min(1.0),
            (dg + sg * op).min(1.0),
            (db + sb * op).min(1.0),
        ),
        state::VisualBlendMode::Screen => (
            1.0 - (1.0 - dr) * (1.0 - sr * op),
            1.0 - (1.0 - dg) * (1.0 - sg * op),
            1.0 - (1.0 - db) * (1.0 - sb * op),
        ),
        state::VisualBlendMode::Multiply => {
            let mr = dr * (sr * op + (1.0 - op));
            let mg = dg * (sg * op + (1.0 - op));
            let mb = db * (sb * op + (1.0 - op));
            (mr, mg, mb)
        }
        state::VisualBlendMode::Maximum => (
            dr.max(sr * op),
            dg.max(sg * op),
            db.max(sb * op),
        ),
        state::VisualBlendMode::Overlay => {
            let overlay_ch = |d: f32, s: f32| -> f32 {
                let blended = if d < 0.5 {
                    2.0 * d * s
                } else {
                    1.0 - 2.0 * (1.0 - d) * (1.0 - s)
                };
                d * (1.0 - op) + blended * op
            };
            (overlay_ch(dr, sr), overlay_ch(dg, sg), overlay_ch(db, sb))
        }
    };

    [
        (out_r.clamp(0.0, 1.0) * 255.0) as u8,
        (out_g.clamp(0.0, 1.0) * 255.0) as u8,
        (out_b.clamp(0.0, 1.0) * 255.0) as u8,
        255,
    ]
}

/// Blend two target screen framebuffers according to a transition type and progress (0.0..=1.0)
pub fn blend_transition_framebuffers(
    from_engine: &state::PixelFeedbackEngine,
    to_engine: &state::PixelFeedbackEngine,
    output_engine: &mut state::PixelFeedbackEngine,
    transition_type: state::ScreenTransitionType,
    progress: f32,
) {
    let p = progress.clamp(0.0, 1.0);
    let w = output_engine.width;
    let h = output_engine.height;

    for y in 0..h {
        let y_ratio = y as f32 / h as f32;
        for x in 0..w {
            let x_ratio = x as f32 / w as f32;
            let idx = y * w + x;

            let pix_from = from_engine.back_buffer.get(idx).copied().unwrap_or([0, 0, 0, 255]);
            let pix_to = to_engine.back_buffer.get(idx).copied().unwrap_or([0, 0, 0, 255]);

            let out_pixel = match transition_type {
                state::ScreenTransitionType::Crossfade => {
                    let r = (pix_from[0] as f32 * (1.0 - p) + pix_to[0] as f32 * p) as u8;
                    let g = (pix_from[1] as f32 * (1.0 - p) + pix_to[1] as f32 * p) as u8;
                    let b = (pix_from[2] as f32 * (1.0 - p) + pix_to[2] as f32 * p) as u8;
                    [r, g, b, 255]
                }
                state::ScreenTransitionType::WipeHorizontal => {
                    if x_ratio < p { pix_to } else { pix_from }
                }
                state::ScreenTransitionType::WipeVertical => {
                    if y_ratio < p { pix_to } else { pix_from }
                }
                state::ScreenTransitionType::GlitchDissolve => {
                    let hash = ((x * 127 + y * 311) % 100) as f32 / 100.0;
                    if hash < p { pix_to } else { pix_from }
                }
                state::ScreenTransitionType::ZoomExpand => {
                    let center_dist = ((x_ratio - 0.5).hypot(y_ratio - 0.5) * 2.0).clamp(0.0, 1.0);
                    if center_dist < p { pix_to } else { pix_from }
                }
            };

            output_engine.back_buffer[idx] = out_pixel;
        }
    }
    output_engine.front_buffer.copy_from_slice(&output_engine.back_buffer);
}

/// Composite a single visual channel framebuffer onto a target screen engine buffer at a given cell rectangle
pub fn composite_channel_onto_screen(
    target_engine: &mut state::PixelFeedbackEngine,
    channel: &state::VisualChannel,
    cell_rect: (usize, usize, usize, usize),
) {
    let (offset_x, offset_y, cell_w, cell_h) = cell_rect;
    if cell_w == 0 || cell_h == 0 {
        return;
    }

    let src_engine = &channel.feedback_engine;
    let src_w = src_engine.width;
    let src_h = src_engine.height;
    if src_w == 0 || src_h == 0 {
        return;
    }

    let tgt_w = target_engine.width;
    let tgt_h = target_engine.height;

    for cy in 0..cell_h {
        let ty = offset_y + cy;
        if ty >= tgt_h {
            break;
        }
        let sy = (cy * src_h) / cell_h;

        for cx in 0..cell_w {
            let tx = offset_x + cx;
            if tx >= tgt_w {
                break;
            }
            let sx = (cx * src_w) / cell_w;

            let src_idx = sy * src_w + sx;
            let tgt_idx = ty * tgt_w + tx;

            let src_pixel = src_engine.back_buffer[src_idx];
            let tgt_pixel = target_engine.back_buffer[tgt_idx];

            target_engine.back_buffer[tgt_idx] = blend_pixel(
                src_pixel,
                tgt_pixel,
                channel.blend_mode,
                channel.opacity,
            );
        }
    }
}

/// Render a composite Target Screen containing all assigned visual channels
pub fn render_composite_target_screen(
    app: &mut InspectorApp,
    target_screen_id: &str,
    ui: &mut egui::Ui,
    _telemetry: &Option<Telemetry>,
) {
    let screen_opt = app.viz.target_screens.iter().find(|s| s.id == target_screen_id).cloned();
    let Some(screen) = screen_opt else { return; };

    // Constrain height to card frame (140px) and compute 16:9 aspect width
    let desired_h = 130.0f32;
    let desired_w = (desired_h * (16.0 / 9.0)).min(ui.available_width());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(desired_w, desired_h), egui::Sense::click_and_drag());

    ui.painter().rect_filled(
        rect,
        0.0,
        egui::Color32::from_rgba_unmultiplied(
            screen.clear_color[0],
            screen.clear_color[1],
            screen.clear_color[2],
            screen.clear_color[3],
        ),
    );

    // Collect matching active/unmuted channels assigned to this target screen
    let mut matching_indices: Vec<usize> = app.viz.channels.iter().enumerate()
        .filter(|(_, c)| c.target_screen_id == target_screen_id && !c.is_muted)
        .map(|(idx, _)| idx)
        .collect();

    // Sort by layer_z_index ascending
    matching_indices.sort_by_key(|&idx| app.viz.channels[idx].layer_z_index);

    if matching_indices.is_empty() {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("Target Screen [{}] — No Visual Channels Routed", screen.name),
            egui::FontId::proportional(14.0),
            egui::Color32::from_rgb(120, 130, 150),
        );
        return;
    }

    // Prepare composite target framebuffer
    let target_w = 320usize;
    let target_h = 200usize;
    let mut target_engine = state::PixelFeedbackEngine::new(target_w, target_h);

    // Initialize with screen clear color
    for pixel in target_engine.back_buffer.iter_mut() {
        *pixel = screen.clear_color;
    }

    // Step each channel and composite into target_engine
    let time = ui.input(|i| i.time);
    let frame_dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.050);

    let num_matching = matching_indices.len();

    for (layer_slot, &c_idx) in matching_indices.iter().enumerate() {
        let channel = &mut app.viz.channels[c_idx];

        // Step audio inputs & nervous system for this channel
        let mut audio_inputs = [0.0f32; 64];
        let low_energy = app.viz.damped_spectrum[0..16].iter().sum::<f32>() / 16.0 * channel.gain_sensitivity;
        let mid_energy = app.viz.damped_spectrum[16..64].iter().sum::<f32>() / 48.0 * channel.gain_sensitivity;
        let high_energy = app.viz.damped_spectrum[64..128].iter().sum::<f32>() / 64.0 * channel.gain_sensitivity;

        for i in 0..32 {
            audio_inputs[i] = app.viz.damped_spectrum[i * 4 % 128] * channel.gain_sensitivity;
        }
        for i in 0..16 {
            audio_inputs[32 + i] = app.viz.damped_goniometer[i * 8 % 128] * channel.gain_sensitivity;
        }
        for i in 0..12 {
            audio_inputs[48 + i] = app.viz.damped_latent[i % 16];
        }
        audio_inputs[60] = low_energy;
        audio_inputs[61] = mid_energy;
        audio_inputs[62] = high_energy;

        channel.nervous_system.rms_energy = low_energy * 0.5 + mid_energy * 0.3 + high_energy * 0.2;
        channel.nervous_system.low_band = low_energy;
        channel.nervous_system.mid_band = mid_energy;
        channel.nervous_system.high_band = high_energy;
        channel.mapper.map(&channel.nervous_system, &mut channel.genome);

        channel.neuron_net.step(&audio_inputs, frame_dt, channel.param_neural_temp, channel.param_feedback);
        let motor = channel.neuron_net.motor_outputs;

        let zoom = 0.98 + low_energy * 0.08;
        let rot = (time * channel.param_speed as f64 * 0.2).sin() as f32 * 0.02 + motor[1] * 0.04;
        let warp_freq = 4.0 + motor[2] * 4.0;
        let decay = (0.88 + channel.param_feedback * 0.10).clamp(0.70, 0.98);

        channel.feedback_engine.step_feedback_warp(
            zoom,
            rot,
            warp_freq,
            decay,
            time as f32,
            &motor,
        );

        apply_visual_insert_chain(
            &mut channel.feedback_engine,
            &channel.visual_inserts,
            channel.param_color_shift,
            time as f32,
        );

        // Determine viewport cell rectangle based on CompositingLayoutMode
        let cell_rect = match screen.layout_mode {
            state::CompositingLayoutMode::LayeredComposite => (0, 0, target_w, target_h),
            state::CompositingLayoutMode::Grid2x2 => {
                let cell_w = target_w / 2;
                let cell_h = target_h / 2;
                match layer_slot {
                    0 => (0, 0, cell_w, cell_h),
                    1 => (cell_w, 0, cell_w, cell_h),
                    2 => (0, cell_h, cell_w, cell_h),
                    _ => (cell_w, cell_h, cell_w, cell_h),
                }
            }
            state::CompositingLayoutMode::SideBySide => {
                let cell_w = target_w / num_matching.max(1);
                let x = (layer_slot * cell_w).min(target_w.saturating_sub(cell_w));
                (x, 0, cell_w, target_h)
            }
            state::CompositingLayoutMode::PictureInPicture => {
                if layer_slot == 0 {
                    (0, 0, target_w, target_h)
                } else {
                    let rx = (channel.viewport_rect[0].clamp(0.0, 1.0) * target_w as f32) as usize;
                    let ry = (channel.viewport_rect[1].clamp(0.0, 1.0) * target_h as f32) as usize;
                    let rw = (channel.viewport_rect[2].clamp(0.1, 1.0) * target_w as f32) as usize;
                    let rh = (channel.viewport_rect[3].clamp(0.1, 1.0) * target_h as f32) as usize;
                    (rx.min(target_w - 1), ry.min(target_h - 1), rw.min(target_w - rx), rh.min(target_h - ry))
                }
            }
        };

        composite_channel_onto_screen(&mut target_engine, channel, cell_rect);
    }

    // Apply active Scene Crossfader transition if active for this target screen
    if app.viz.screen_transition.is_active && app.viz.screen_transition.to_screen_id == target_screen_id {
        let from_screen_id = app.viz.screen_transition.from_screen_id.clone();
        let trans_type = app.viz.screen_transition.transition_type;
        let trans_progress = app.viz.screen_transition.progress;

        let mut from_engine = state::PixelFeedbackEngine::new(target_w, target_h);
        for pixel in from_engine.back_buffer.iter_mut() {
            *pixel = screen.clear_color;
        }

        // Composite from_screen channels
        let mut from_indices: Vec<usize> = app.viz.channels.iter().enumerate()
            .filter(|(_, c)| c.target_screen_id == from_screen_id && !c.is_muted)
            .map(|(idx, _)| idx)
            .collect();
        from_indices.sort_by_key(|&idx| app.viz.channels[idx].layer_z_index);

        for (_from_slot, &c_idx) in from_indices.iter().enumerate() {
            let channel = &app.viz.channels[c_idx];
            let cell_rect = (0, 0, target_w, target_h);
            composite_channel_onto_screen(&mut from_engine, channel, cell_rect);
        }

        let mut blended_engine = state::PixelFeedbackEngine::new(target_w, target_h);
        blend_transition_framebuffers(&from_engine, &target_engine, &mut blended_engine, trans_type, trans_progress);
        target_engine = blended_engine;
    }

    target_engine.front_buffer.copy_from_slice(&target_engine.back_buffer);

    let color_image = egui::ColorImage::from_rgba_unmultiplied(
        [target_engine.width, target_engine.height],
        target_engine.front_buffer.as_flattened(),
    );

    let texture_handle = ui.ctx().load_texture(
        format!("composite_target_screen_{}", target_screen_id),
        color_image,
        egui::TextureOptions::LINEAR,
    );

    ui.painter().image(
        texture_handle.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );

    // Interactive Drag-and-Drop Viewport Placement Handles for PictureInPicture Mode
    if screen.layout_mode == state::CompositingLayoutMode::PictureInPicture && matching_indices.len() > 1 {
        for (layer_slot, &c_idx) in matching_indices.iter().enumerate().skip(1) {
            let channel = &mut app.viz.channels[c_idx];
            let vx = channel.viewport_rect[0].clamp(0.0, 0.9);
            let vy = channel.viewport_rect[1].clamp(0.0, 0.9);
            let vw = channel.viewport_rect[2].clamp(0.1, 1.0);
            let vh = channel.viewport_rect[3].clamp(0.1, 1.0);

            let pip_min = rect.min + egui::vec2(vx * rect.width(), vy * rect.height());
            let pip_max = pip_min + egui::vec2(vw * rect.width(), vh * rect.height());
            let pip_rect = egui::Rect::from_min_max(pip_min, pip_max);

            // Handle Pointer Drag on PIP Canvas Box
            if response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let norm_x = ((pos.x - rect.left() - pip_rect.width() * 0.5) / rect.width()).clamp(0.0, 1.0 - vw);
                    let norm_y = ((pos.y - rect.top() - pip_rect.height() * 0.5) / rect.height()).clamp(0.0, 1.0 - vh);
                    channel.viewport_rect[0] = norm_x;
                    channel.viewport_rect[1] = norm_y;
                }
            }

            // Draw PIP interactive viewport bounding box with handle handles
            let stroke_color = app.theme.deck_colors[(layer_slot - 1) % 4];
            ui.painter().rect_stroke(pip_rect, 2.0, egui::Stroke::new(2.0, stroke_color));
            ui.painter().rect_filled(
                egui::Rect::from_center_size(pip_rect.right_bottom(), egui::vec2(8.0, 8.0)),
                2.0,
                stroke_color,
            );
            ui.painter().text(
                pip_rect.left_top() + egui::vec2(4.0, 4.0),
                egui::Align2::LEFT_TOP,
                format!("PIP L{}: {}", layer_slot, channel.name),
                egui::FontId::proportional(10.0),
                stroke_color,
            );
        }
    }
}

/// Render detached interactive visual surface
pub fn render_detached_interactive_surface(
    app: &mut InspectorApp,
    channel_idx: usize,
    _ctx: &egui::Context,
    ui: &mut egui::Ui,
    telemetry: &Option<Telemetry>,
) {
    let target_idx = channel_idx.min(app.viz.channels.len().saturating_sub(1));

    if let Some(channel) = app.viz.channels.get_mut(target_idx) {
        let available_size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(available_size.max(egui::vec2(200.0, 200.0)), egui::Sense::drag());

        ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgb(10, 12, 18));

        let time = ui.input(|i| i.time) * channel.param_speed as f64;

        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let norm_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let norm_y = ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
                channel.param_neural_temp = norm_x * 2.5;
                channel.param_color_shift = norm_y;
            }
        }

        let mut _input_energy = 0.0f32;
        let mut stereo_imbalance = 0.0f32;
        if let Some(t) = telemetry {
            _input_energy = t.peak_levels.first().copied().unwrap_or(0.0) * channel.gain_sensitivity;
            stereo_imbalance = app.viz.damped_goniometer.iter().sum::<f32>() / 128.0;
        }

        let low_energy = app.viz.damped_spectrum[0..16].iter().sum::<f32>() / 16.0 * channel.gain_sensitivity;
        let mid_energy = app.viz.damped_spectrum[16..64].iter().sum::<f32>() / 48.0 * channel.gain_sensitivity;
        let high_energy = app.viz.damped_spectrum[64..128].iter().sum::<f32>() / 64.0 * channel.gain_sensitivity;

        let un_damped_peak = telemetry.as_ref().and_then(|t| t.peak_levels.first().copied()).unwrap_or(0.0) * channel.gain_sensitivity;
        let fast_attack = (un_damped_peak - channel.nervous_system.short_term_envelope).max(0.0) * 3.0;

        channel.nervous_system.rms_energy = low_energy * 0.5 + mid_energy * 0.3 + high_energy * 0.2;
        channel.nervous_system.spectral_centroid = (mid_energy * 1000.0 + high_energy * 4000.0) / (low_energy + mid_energy + high_energy + 0.001);
        channel.nervous_system.spectral_flux = (high_energy - low_energy).abs();
        channel.nervous_system.low_band = low_energy;
        channel.nervous_system.mid_band = mid_energy;
        channel.nervous_system.high_band = high_energy;
        channel.nervous_system.transient_density = (high_energy * 2.5 + fast_attack).clamp(0.0, 1.0);
        channel.nervous_system.onset_strength = (low_energy * 3.0 + fast_attack * 2.0).clamp(0.0, 1.0);
        channel.nervous_system.fast_transient_spike = fast_attack.clamp(0.0, 2.0);
        channel.nervous_system.bpm = telemetry.as_ref().map(|t| t.bpm as f32).unwrap_or(120.0);
        channel.nervous_system.beat_phase = telemetry.as_ref().map(|t| t.beat_position as f32 % 1.0).unwrap_or(0.0);
        channel.nervous_system.sub_beat_phase = (channel.nervous_system.beat_phase * 4.0).fract();
        for i in 0..12 {
            channel.nervous_system.pitch_chroma[i] = app.viz.damped_spectrum[i * 10 % 128];
        }
        channel.nervous_system.harmonicity = (mid_energy / (high_energy + 0.001)).clamp(0.0, 1.0);
        channel.nervous_system.noisiness = (high_energy / (low_energy + 0.001)).clamp(0.0, 1.0);
        channel.nervous_system.stereo_width = (channel.stereo_width * (1.0 + stereo_imbalance.abs())).clamp(0.0, 2.0);
        channel.nervous_system.stereo_asymmetry = stereo_imbalance;
        channel.nervous_system.long_term_envelope = app.viz.damped_peaks.iter().sum::<f32>() / 16.0;
        channel.nervous_system.short_term_envelope = un_damped_peak;
        channel.nervous_system.spectral_entropy = (low_energy * mid_energy * high_energy).powf(0.33);
        channel.nervous_system.zero_crossing_rate = high_energy * 0.8;

        let current_time = ui.input(|i| i.time);
        channel.mapper.map(&channel.nervous_system, &mut channel.genome);
        channel.memory.push_snapshot(channel.nervous_system.rms_energy, channel.nervous_system.spectral_centroid, current_time);
        channel.mutation.handle_event(&channel.nervous_system, &channel.memory);

        let mut audio_inputs = [0.0f32; 64];
        for i in 0..32 {
            audio_inputs[i] = app.viz.damped_spectrum[i * 4 % 128] * channel.gain_sensitivity;
        }
        for i in 0..16 {
            audio_inputs[32 + i] = app.viz.damped_goniometer[i * 8 % 128] * channel.gain_sensitivity;
        }
        for i in 0..12 {
            audio_inputs[48 + i] = app.viz.damped_latent[i % 16];
        }
        audio_inputs[60] = low_energy;
        audio_inputs[61] = mid_energy;
        audio_inputs[62] = high_energy;
        audio_inputs[63] = stereo_imbalance;

        let frame_dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.050);
        channel.neuron_net.step(&audio_inputs, frame_dt, channel.param_neural_temp, channel.param_feedback);

        let motor = channel.neuron_net.motor_outputs;

        let zoom = 0.98 + (channel.nervous_system.low_band * 0.08) + channel.nervous_system.fast_transient_spike * 0.05;
        let rot = (time as f32 * 0.2 * channel.param_speed).sin() * 0.02 + motor[1] * 0.04;
        let warp_freq = 4.0 + motor[2] * 4.0;
        let decay = (0.88 + channel.param_feedback * 0.10).clamp(0.70, 0.98);

        channel.feedback_engine.step_feedback_warp(
            zoom,
            rot,
            warp_freq,
            decay,
            time as f32,
            &motor,
        );

        let fb_w = channel.feedback_engine.width as f32;
        let fb_h = channel.feedback_engine.height as f32;
        let fb_center_x = fb_w * 0.5;
        let fb_center_y = fb_h * 0.5;

        let num_fb_pts = 48;
        for i in 0..num_fb_pts - 1 {
            let t1 = (i as f32 / num_fb_pts as f32) * std::f32::consts::TAU;
            let t2 = ((i + 1) as f32 / num_fb_pts as f32) * std::f32::consts::TAU;

            let r1 = (fb_h * 0.38) * (1.0 + (channel.neuron_net.v[i % 64] + 65.0) / 100.0);
            let r2 = (fb_h * 0.38) * (1.0 + (channel.neuron_net.v[(i + 1) % 64] + 65.0) / 100.0);

            let x0 = fb_center_x + (t1 * 3.0 + time as f32).sin() * r1;
            let y0 = fb_center_y + (t1 * 2.0 + time as f32).cos() * r1;
            let x1 = fb_center_x + (t2 * 3.0 + time as f32).sin() * r2;
            let y1 = fb_center_y + (t2 * 2.0 + time as f32).cos() * r2;

            let color = if channel.neuron_net.spikes[i % 64] {
                [255, 220, 80]
            } else {
                [
                    ((i * 12) % 255) as u8,
                    ((255 - i * 8) % 255) as u8,
                    220,
                ]
            };

            channel.feedback_engine.draw_line_additive(x0, y0, x1, y1, color);
        }

        // Chain visual post-processing inserts onto PixelFeedbackEngine framebuffers
        apply_visual_insert_chain(
            &mut channel.feedback_engine,
            &channel.visual_inserts,
            channel.param_color_shift,
            time as f32,
        );

        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [channel.feedback_engine.width, channel.feedback_engine.height],
            channel.feedback_engine.front_buffer.as_flattened(),
        );

        let texture_handle = ui.ctx().load_texture(
            format!("fb_tex_{}", target_idx),
            color_image,
            egui::TextureOptions::LINEAR,
        );

        ui.painter().image(
            texture_handle.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        use crate::views::visual_engines::NeuralVisualEngine;

        let nervous = &channel.nervous_system;
        let genome = &channel.genome;

        match channel.generator {
            state::VisualGenerator::RadialMandala => channel.engine_radial_mandala.render(ui, rect, nervous, genome, telemetry, time as f32),
            state::VisualGenerator::LiquidSurface => channel.engine_liquid_surface.render(ui, rect, nervous, genome, telemetry, time as f32),
            state::VisualGenerator::SpectralLandscape => channel.engine_spectral_landscape.render(ui, rect, nervous, genome, telemetry, time as f32),
            state::VisualGenerator::HyperAttractor => channel.engine_hyper_attractor.render(ui, rect, nervous, genome, telemetry, time as f32),
            state::VisualGenerator::ReactionDiffusion => channel.engine_reaction_diffusion.render(ui, rect, nervous, genome, telemetry, time as f32),
            state::VisualGenerator::NeuralRaymarcher => channel.engine_neural_raymarcher.render(ui, rect, nervous, genome, telemetry, time as f32),
            state::VisualGenerator::NeuralNcaMesh => channel.engine_neural_nca_mesh.render(ui, rect, nervous, genome, telemetry, time as f32),
        }
    }
}

/// Render the Visual Mixer View page
pub fn render_visuals_view(app: &mut InspectorApp, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
    const VIZ_STRIP_W: f32 = 140.0;
    const VIZ_FADER_H: f32 = 150.0;
    const VIZ_PREVIEW_H: f32 = 225.0;

    let theme = app.theme.clone();

    ui.heading(egui::RichText::new("Visual Mixer & Target Screens").size(theme.type_heading));
    ui.add_space(theme.space_xs);

    // Target Screens / Compositor Top Section
    egui::Frame::none()
        .fill(theme.bg_surface)
        .rounding(egui::Rounding::same(theme.radius_md))
        .inner_margin(egui::Margin::same(theme.space_md))
        .stroke(egui::Stroke::new(1.0, theme.border))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("TARGET SCREENS & COMPOSITOR").size(theme.type_body).strong().color(theme.accent));
                ui.add_space(12.0);

                let num_screens = app.viz.target_screens.len();
                if app.viz.active_target_screen_idx >= num_screens {
                    app.viz.active_target_screen_idx = num_screens.saturating_sub(1);
                }

                for s_idx in 0..num_screens {
                    let is_sel = app.viz.active_target_screen_idx == s_idx;
                    let screen = &app.viz.target_screens[s_idx];
                    if ui.selectable_label(is_sel, egui::RichText::new(&screen.name).strong()).clicked() {
                        app.viz.active_target_screen_idx = s_idx;
                    }
                }
            });

            ui.add_space(4.0);

            if app.viz.active_target_screen_idx < app.viz.target_screens.len() {
                let active_idx = app.viz.active_target_screen_idx;
                let active_id = app.viz.target_screens[active_idx].id.clone();

                ui.group(|ui| {
                    ui.set_height(140.0);
                    render_composite_target_screen(app, &active_id, ui, telemetry);
                });
            }
        });

    ui.add_space(theme.space_md);

    // Visual Channel Strips inside horizontal ScrollArea (matching System Mixer)
    ui.horizontal_top(|ui| {
        egui::ScrollArea::horizontal()
            .id_source("visual_mixer_scroll")
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let num_channels = app.viz.channels.len();
                    let mut channel_to_remove = None;

                    for c_idx in 0..num_channels {
                        let channel = &mut app.viz.channels[c_idx];
                        let is_selected = app.viz.selected_channel_idx == c_idx;
                        let channel_color = theme.deck_colors[c_idx % 4];

                        egui::Frame::none()
                            .fill(theme.bg_surface)
                            .rounding(egui::Rounding::same(theme.radius_md))
                            .inner_margin(egui::Margin::same(theme.space_md))
                            .stroke(egui::Stroke::new(1.0, if is_selected { theme.accent } else { theme.border }))
                            .show(ui, |ui| {
                                ui.set_width(VIZ_STRIP_W);
                                ui.vertical(|ui| {
                                    // Header
                                    ui.horizontal(|ui| {
                                        ui.add_space((VIZ_STRIP_W - 50.0).max(0.0) / 2.0);
                                        if ui.button(egui::RichText::new(&channel.name).strong().size(theme.type_body).color(channel_color)).clicked() {
                                            app.viz.selected_channel_idx = c_idx;
                                        }
                                        if num_channels > 1 {
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if ui.button(egui_phosphor::regular::X).on_hover_text("Remove Strip").clicked() {
                                                    channel_to_remove = Some(c_idx);
                                                }
                                            });
                                        }
                                    });
                                    ui.add_space(theme.space_xs);

                                    // Generator Dropdown Selector
                                    ui.horizontal(|ui| {
                                        let avail_w = ui.available_width();
                                        egui::ComboBox::from_id_source(format!("gen_combo_{}", c_idx))
                                            .selected_text(egui::RichText::new(channel.generator.name()).size(8.5).strong().color(theme.text_primary))
                                            .width(avail_w)
                                            .show_ui(ui, |ui| {
                                                for generator_item in state::VisualGenerator::all() {
                                                    ui.selectable_value(&mut channel.generator, generator_item.clone(), generator_item.name());
                                                }
                                            });
                                    });
                                    ui.add_space(2.0);

                                    // Visual Surface Live Preview Frame
                                    let (rect, _resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), VIZ_PREVIEW_H), egui::Sense::click());
                                    ui.painter().rect_filled(rect, theme.radius_sm, theme.bg_inset);
                                    ui.painter().rect_stroke(rect, theme.radius_sm, egui::Stroke::new(1.0, theme.border_stroke.color));

                                    // Step audio telemetry and render live visual surface
                                    let time = ui.input(|i| i.time) * channel.param_speed as f64;
                                    let low_energy = app.viz.damped_spectrum[0..16].iter().sum::<f32>() / 16.0 * channel.gain_sensitivity;
                                    let mid_energy = app.viz.damped_spectrum[16..64].iter().sum::<f32>() / 48.0 * channel.gain_sensitivity;
                                    let high_energy = app.viz.damped_spectrum[64..128].iter().sum::<f32>() / 64.0 * channel.gain_sensitivity;

                                    channel.nervous_system.rms_energy = low_energy * 0.5 + mid_energy * 0.3 + high_energy * 0.2;
                                    channel.nervous_system.low_band = low_energy;
                                    channel.nervous_system.mid_band = mid_energy;
                                    channel.nervous_system.high_band = high_energy;
                                    channel.mapper.map(&channel.nervous_system, &mut channel.genome);

                                    let mut audio_inputs = [0.0f32; 64];
                                    for idx in 0..32 {
                                        audio_inputs[idx] = app.viz.damped_spectrum[idx * 4 % 128] * channel.gain_sensitivity;
                                    }
                                    let frame_dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.050);
                                    channel.neuron_net.step(&audio_inputs, frame_dt, channel.param_neural_temp, channel.param_feedback);
                                    let motor = channel.neuron_net.motor_outputs;

                                    let zoom = 0.98 + (channel.nervous_system.low_band * 0.08);
                                    let rot = (time as f32 * 0.2 * channel.param_speed).sin() * 0.02 + motor[1] * 0.04;
                                    let warp_freq = 4.0 + motor[2] * 4.0;
                                    let decay = (0.88 + channel.param_feedback * 0.10).clamp(0.70, 0.98);

                                    channel.feedback_engine.step_feedback_warp(zoom, rot, warp_freq, decay, time as f32, &motor);
                                    apply_visual_insert_chain(&mut channel.feedback_engine, &channel.visual_inserts, channel.param_color_shift, time as f32);

                                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                        [channel.feedback_engine.width, channel.feedback_engine.height],
                                        channel.feedback_engine.front_buffer.as_flattened(),
                                    );
                                    let texture_handle = ui.ctx().load_texture(
                                        format!("strip_preview_tex_{}", c_idx),
                                        color_image,
                                        egui::TextureOptions::LINEAR,
                                    );
                                    ui.painter().image(
                                        texture_handle.id(),
                                        rect,
                                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                        egui::Color32::WHITE,
                                    );

                                    use crate::views::visual_engines::NeuralVisualEngine;
                                    let nervous = &channel.nervous_system;
                                    let genome = &channel.genome;
                                    match channel.generator {
                                        state::VisualGenerator::RadialMandala => channel.engine_radial_mandala.render(ui, rect, nervous, genome, telemetry, time as f32),
                                        state::VisualGenerator::LiquidSurface => channel.engine_liquid_surface.render(ui, rect, nervous, genome, telemetry, time as f32),
                                        state::VisualGenerator::SpectralLandscape => channel.engine_spectral_landscape.render(ui, rect, nervous, genome, telemetry, time as f32),
                                        state::VisualGenerator::HyperAttractor => channel.engine_hyper_attractor.render(ui, rect, nervous, genome, telemetry, time as f32),
                                        state::VisualGenerator::ReactionDiffusion => channel.engine_reaction_diffusion.render(ui, rect, nervous, genome, telemetry, time as f32),
                                        state::VisualGenerator::NeuralRaymarcher => channel.engine_neural_raymarcher.render(ui, rect, nervous, genome, telemetry, time as f32),
                                        state::VisualGenerator::NeuralNcaMesh => channel.engine_neural_nca_mesh.render(ui, rect, nervous, genome, telemetry, time as f32),
                                    }

                                        // --- VISUAL INSERTS RACK ---
                                        ui.group(|ui| {
                                            ui.set_width(VIZ_STRIP_W - 12.0);
                                            ui.vertical_centered(|ui| {
                                                ui.label(egui::RichText::new("CHANNEL STRIP").size(theme.type_caption).strong().color(theme.text_secondary));
                                                ui.add_space(2.0);

                                                // Parameter Knobs Card
                                                egui::Frame::none()
                                                    .fill(theme.bg_inset)
                                                    .rounding(egui::Rounding::same(theme.radius_sm))
                                                    .inner_margin(egui::Margin::same(4.0))
                                                    .stroke(egui::Stroke::new(1.0, theme.border_stroke.color))
                                                    .show(ui, |ui| {
                                                        ui.set_width(VIZ_STRIP_W - 20.0);
                                                        ui.vertical_centered(|ui| {
                                                            ui.label(egui::RichText::new("PARAMS").size(9.0).strong().color(theme.accent));
                                                            ui.horizontal(|ui| {
                                                                ui.spacing_mut().item_spacing.x = 2.0;
                                                                nullherz_ui_hal::widgets::render_knob_sized(ui, &mut channel.param_speed, 0.1..=4.0, "SPD", channel_color, 24.0);
                                                                nullherz_ui_hal::widgets::render_knob_sized(ui, &mut channel.param_neural_temp, 0.0..=2.0, "TMP", channel_color, 24.0);
                                                                nullherz_ui_hal::widgets::render_knob_sized(ui, &mut channel.param_feedback, 0.0..=1.0, "FB", channel_color, 24.0);
                                                            });
                                                        });
                                                    });

                                                ui.add_space(4.0);

                                                // Display list of chained visual insert sidecar slots with top-right '×' and up/down buttons
                                                let mut insert_to_remove = None;
                                                let mut insert_to_move_up = None;
                                                let mut insert_to_move_down = None;
                                                let total_visual_inserts = channel.visual_inserts.len();

                                                for (ins_idx, ins_id) in channel.visual_inserts.iter().enumerate() {
                                                    let ins_name = ins_id.clone();
                                                    egui::Frame::none()
                                                        .fill(theme.accent.linear_multiply(0.15))
                                                        .rounding(egui::Rounding::same(theme.radius_sm))
                                                        .inner_margin(egui::Margin::same(4.0))
                                                        .stroke(egui::Stroke::new(1.0, theme.accent))
                                                        .show(ui, |ui| {
                                                            ui.set_width(VIZ_STRIP_W - 20.0);
                                                            ui.vertical(|ui| {
                                                                ui.horizontal(|ui| {
                                                                    ui.set_width(ui.available_width());
                                                                    ui.label(egui::RichText::new(format!("FX: {}", ins_name)).size(9.0).strong().color(theme.text_primary));
                                                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                                        let close_btn = egui::Button::new(egui::RichText::new("×").size(11.0).strong().color(theme.danger))
                                                                            .fill(theme.bg_inset)
                                                                            .min_size(egui::vec2(16.0, 16.0));
                                                                        if ui.add(close_btn).on_hover_text("Remove Visual FX").clicked() {
                                                                            insert_to_remove = Some(ins_idx);
                                                                        }
                                                                        if ins_idx < total_visual_inserts - 1 {
                                                                            let dn_btn = egui::Button::new(egui::RichText::new("▼").size(9.0).strong().color(theme.text_secondary))
                                                                                .fill(theme.bg_inset)
                                                                                .min_size(egui::vec2(14.0, 14.0));
                                                                            if ui.add(dn_btn).on_hover_text("Move Down").clicked() {
                                                                                insert_to_move_down = Some(ins_idx);
                                                                            }
                                                                        }
                                                                        if ins_idx > 0 {
                                                                            let up_btn = egui::Button::new(egui::RichText::new("▲").size(9.0).strong().color(theme.text_secondary))
                                                                                .fill(theme.bg_inset)
                                                                                .min_size(egui::vec2(14.0, 14.0));
                                                                            if ui.add(up_btn).on_hover_text("Move Up").clicked() {
                                                                                insert_to_move_up = Some(ins_idx);
                                                                            }
                                                                        }
                                                                    });
                                                                });
                                                            });
                                                        });
                                                    ui.add_space(4.0);
                                                }

                                                if let Some(idx) = insert_to_move_up {
                                                    if idx > 0 && idx < channel.visual_inserts.len() {
                                                        channel.visual_inserts.swap(idx, idx - 1);
                                                    }
                                                }
                                                if let Some(idx) = insert_to_move_down {
                                                    if idx + 1 < channel.visual_inserts.len() {
                                                        channel.visual_inserts.swap(idx, idx + 1);
                                                    }
                                                }
                                                if let Some(rem_idx) = insert_to_remove {
                                                    if rem_idx < channel.visual_inserts.len() {
                                                        channel.visual_inserts.remove(rem_idx);
                                                    }
                                                }

                                                if ui.add_sized([VIZ_STRIP_W - 20.0, 18.0], egui::Button::new(egui::RichText::new("+ VISUAL FX").size(9.0).strong()).fill(theme.bg_inset)).clicked() {
                                                    app.active_right_tab = Some(crate::RightTab::Store);
                                                    app.store.active_tag_filter = Some("visual".to_string());
                                                }
                                            });
                                        });

                                        ui.add_space(theme.space_md);

                                        // --- SENSITIVITY FADER & STEREO REACTIVITY VU METERS ---
                                        ui.horizontal(|ui| {
                                            let fader_w = 24.0;
                                            let pad = (ui.available_width() - fader_w).max(0.0) / 2.0;
                                            ui.add_space(pad);

                                            nullherz_ui_hal::widgets::render_fader(ui, &mut channel.gain_sensitivity, 0.0..=2.0, channel_color, VIZ_FADER_H, 30.0);
                                            ui.add_space(4.0);

                                            let mut lvl_l = 0.0f32;
                                            let mut lvl_r = 0.0f32;
                                            if let Some(t) = telemetry {
                                                lvl_l = t.peak_levels.first().copied().unwrap_or(0.0) * channel.gain_sensitivity;
                                                lvl_r = t.peak_levels.get(1).copied().unwrap_or(lvl_l) * channel.gain_sensitivity;
                                            }

                                            nullherz_ui_hal::widgets::render_vu_meter(ui, lvl_l, lvl_r, channel_color, VIZ_FADER_H);
                                        });

                                        ui.add_space(theme.space_xs);

                                        // Transport Row: Mute / Solo / Detached Window buttons
                                        ui.horizontal(|ui| {
                                            ui.add_space((VIZ_STRIP_W - 2.0 * theme.space_md - 96.0).max(0.0) / 2.0);

                                            if ui.add_sized([30.0, 22.0], egui::SelectableLabel::new(channel.is_muted, "M")).clicked() {
                                                channel.is_muted = !channel.is_muted;
                                            }
                                            if ui.add_sized([30.0, 22.0], egui::SelectableLabel::new(channel.is_solo, "S")).clicked() {
                                                channel.is_solo = !channel.is_solo;
                                            }

                                            let is_detached = app.viz.detached_channel == Some(c_idx);
                                            let detach_icon = if is_detached {
                                                egui_phosphor::regular::ARROWS_IN_SIMPLE
                                            } else {
                                                egui_phosphor::regular::ARROWS_OUT_SIMPLE
                                            };

                                            if ui.add_sized([30.0, 22.0], egui::Button::new(egui::RichText::new(detach_icon).size(12.0).strong()).fill(theme.bg_inset)).clicked() {
                                                if is_detached {
                                                    app.viz.detached_channel = None;
                                                } else {
                                                    app.viz.detached_channel = Some(c_idx);
                                                }
                                            }
                                        });
                                    });
                                });

                            ui.add_space(theme.space_sm);
                        }

                        if let Some(idx_to_remove) = channel_to_remove {
                            if idx_to_remove < app.viz.channels.len() {
                                app.viz.channels.remove(idx_to_remove);
                                if app.viz.selected_channel_idx >= app.viz.channels.len() {
                                    app.viz.selected_channel_idx = app.viz.channels.len().saturating_sub(1);
                                }
                            }
                        }

                        // Add "+" channel button
                        egui::Frame::none()
                            .fill(theme.bg_surface)
                            .rounding(egui::Rounding::same(theme.radius_md))
                            .inner_margin(egui::Margin::same(theme.space_md))
                            .stroke(egui::Stroke::new(1.0, theme.border))
                            .show(ui, |ui| {
                                ui.set_width(VIZ_STRIP_W);
                                ui.vertical_centered(|ui| {
                                    ui.add_space(180.0);
                                    let btn = egui::Button::new(egui::RichText::new("+").size(24.0).strong().color(theme.accent))
                                        .fill(theme.bg_inset)
                                        .min_size(egui::vec2(50.0, 50.0));
                                    if ui.add(btn).on_hover_text("Add Visual Strip").clicked() {
                                        let count = app.viz.channels.len() + 1;
                                        app.viz.channels.push(state::VisualChannel::new(
                                            &format!("VIZ {}", count),
                                            state::VisualGenerator::RadialMandala,
                                            vec![state::VisualInputSource::MasterMix],
                                        ));
                                    }
                                    ui.add_space(4.0);
                                    ui.label(egui::RichText::new("ADD VISUAL STRIP").size(9.0).strong().color(theme.text_secondary));
                                });
                            });
                    });
                });
        });

        ui.add_space(theme.space_md);

    ui.add_space(theme.space_md);

    // Target Screens / Compositor Section
    egui::Frame::none()
        .fill(theme.bg_surface)
        .rounding(egui::Rounding::same(theme.radius_md))
        .inner_margin(egui::Margin::same(theme.space_md))
        .stroke(egui::Stroke::new(1.0, theme.border))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("TARGET SCREENS & COMPOSITOR").size(theme.type_body).strong().color(theme.accent));
                ui.add_space(12.0);

                let num_screens = app.viz.target_screens.len();
                if app.viz.active_target_screen_idx >= num_screens {
                    app.viz.active_target_screen_idx = num_screens.saturating_sub(1);
                }

                for s_idx in 0..num_screens {
                    let is_sel = app.viz.active_target_screen_idx == s_idx;
                    let screen = &app.viz.target_screens[s_idx];
                    if ui.selectable_label(is_sel, egui::RichText::new(&screen.name).strong()).clicked() {
                        app.viz.active_target_screen_idx = s_idx;
                    }
                }
            });

            ui.add_space(4.0);

            if app.viz.active_target_screen_idx < app.viz.target_screens.len() {
                let active_idx = app.viz.active_target_screen_idx;
                let active_id = app.viz.target_screens[active_idx].id.clone();

                ui.group(|ui| {
                    ui.set_height(140.0);
                    render_composite_target_screen(app, &active_id, ui, telemetry);
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visual_channel_inserts_initialization() {
        let channel = state::VisualChannel::new(
            "VIZ TEST",
            state::VisualGenerator::RadialMandala,
            vec![state::VisualInputSource::MasterMix],
        );

        assert!(!channel.visual_inserts.is_empty());
        assert!(channel.visual_inserts.contains(&"phase-goniometer-2d".to_string()));
        assert!(channel.visual_inserts.contains(&"fft-spectrum-mesh".to_string()));
    }

    #[test]
    fn test_apply_visual_insert_chain_framebuffer_processing() {
        let mut engine = state::PixelFeedbackEngine::new(32, 32);
        let initial_pixels = engine.back_buffer.clone();

        let inserts = vec![
            "phase-goniometer-2d".to_string(),
            "neural-saturation".to_string(),
            "bloom-filter".to_string(),
            "fft-spectrum-mesh".to_string(),
            "reaction-diffusion-nn".to_string(),
        ];

        apply_visual_insert_chain(&mut engine, &inserts, 0.5, 1.0);

        // Framebuffer pixels must be modified by post-processing inserts
        assert_ne!(engine.back_buffer, initial_pixels);
        assert_eq!(engine.front_buffer, engine.back_buffer);
    }

    #[test]
    fn test_blend_pixel_modes() {
        let src = [100, 150, 200, 255];
        let dst = [50, 50, 50, 255];

        // Additive
        let add = blend_pixel(src, dst, state::VisualBlendMode::Additive, 1.0);
        assert_eq!(add, [150, 200, 250, 255]);

        // Maximum
        let max = blend_pixel(src, dst, state::VisualBlendMode::Maximum, 1.0);
        assert_eq!(max, [100, 150, 200, 255]);

        // Normal (Alpha 1.0)
        let norm = blend_pixel(src, dst, state::VisualBlendMode::Normal, 1.0);
        assert_eq!(norm, [100, 150, 200, 255]);

        // Zero opacity returns dst
        let zero_op = blend_pixel(src, dst, state::VisualBlendMode::Additive, 0.0);
        assert_eq!(zero_op, dst);
    }

    #[test]
    fn test_composite_channel_onto_screen() {
        let mut target_engine = state::PixelFeedbackEngine::new(32, 32);
        for pixel in target_engine.back_buffer.iter_mut() {
            *pixel = [0, 0, 0, 255];
        }

        let mut channel = state::VisualChannel::new(
            "VIZ STEM 1",
            state::VisualGenerator::RadialMandala,
            vec![state::VisualInputSource::DeckA],
        );
        channel.blend_mode = state::VisualBlendMode::Additive;
        channel.opacity = 1.0;
        for pixel in channel.feedback_engine.back_buffer.iter_mut() {
            *pixel = [50, 100, 150, 255];
        }

        // Composite onto full screen
        composite_channel_onto_screen(&mut target_engine, &channel, (0, 0, 32, 32));

        assert_eq!(target_engine.back_buffer[0], [50, 100, 150, 255]);
        assert_eq!(target_engine.back_buffer[32 * 16 + 16], [50, 100, 150, 255]);
    }

    #[test]
    fn test_target_screens_routing_and_detaching() {
        let mut viz_state = state::VizState::default();
        assert_eq!(viz_state.target_screens.len(), 2);

        let screen3 = state::VisualTargetScreen::new("screen_3", "Projection Screen 3", state::CompositingLayoutMode::Grid2x2);
        viz_state.target_screens.push(screen3);
        assert_eq!(viz_state.target_screens.len(), 3);

        // Assign channel 0 to screen_3
        viz_state.channels[0].target_screen_id = "screen_3".to_string();
        assert_eq!(viz_state.channels[0].target_screen_id, "screen_3");

        // Detach screen_3
        viz_state.detached_target_screens.insert("screen_3".to_string());
        assert!(viz_state.detached_target_screens.contains("screen_3"));
    }

    #[test]
    fn test_pip_viewport_drag_bounds() {
        let mut channel = state::VisualChannel::new(
            "PIP TEST",
            state::VisualGenerator::RadialMandala,
            vec![state::VisualInputSource::DeckA],
        );
        // Default viewport_rect is [0.0, 0.0, 1.0, 1.0]
        assert_eq!(channel.viewport_rect, [0.0, 0.0, 1.0, 1.0]);

        // Drag to custom PIP location
        channel.viewport_rect[0] = 0.6;
        channel.viewport_rect[1] = 0.6;
        channel.viewport_rect[2] = 0.35;
        channel.viewport_rect[3] = 0.35;

        let norm_x = channel.viewport_rect[0].clamp(0.0, 1.0 - channel.viewport_rect[2]);
        let norm_y = channel.viewport_rect[1].clamp(0.0, 1.0 - channel.viewport_rect[3]);

        assert!((norm_x - 0.60).abs() < 0.01);
        assert!((norm_y - 0.60).abs() < 0.01);
    }

    #[test]
    fn test_screen_transition_interpolation() {
        let mut from_engine = state::PixelFeedbackEngine::new(16, 16);
        for pixel in from_engine.back_buffer.iter_mut() {
            *pixel = [200, 0, 0, 255];
        }

        let mut to_engine = state::PixelFeedbackEngine::new(16, 16);
        for pixel in to_engine.back_buffer.iter_mut() {
            *pixel = [0, 200, 0, 255];
        }

        let mut out_engine = state::PixelFeedbackEngine::new(16, 16);

        // At progress 0.0, output matches from_engine
        blend_transition_framebuffers(&from_engine, &to_engine, &mut out_engine, state::ScreenTransitionType::Crossfade, 0.0);
        assert_eq!(out_engine.back_buffer[0], [200, 0, 0, 255]);

        // At progress 1.0, output matches to_engine
        blend_transition_framebuffers(&from_engine, &to_engine, &mut out_engine, state::ScreenTransitionType::Crossfade, 1.0);
        assert_eq!(out_engine.back_buffer[0], [0, 200, 0, 255]);

        // At progress 0.5, crossfade yields 50/50 blend
        blend_transition_framebuffers(&from_engine, &to_engine, &mut out_engine, state::ScreenTransitionType::Crossfade, 0.5);
        assert_eq!(out_engine.back_buffer[0], [100, 100, 0, 255]);
    }

    #[test]
    fn test_network_stream_protocol_selection() {
        let mut screen = state::VisualTargetScreen::new("screen_ndi", "NDI Output Screen", state::CompositingLayoutMode::LayeredComposite);
        assert_eq!(screen.network_protocol, state::NetworkStreamProtocol::LocalViewport);

        screen.network_protocol = state::NetworkStreamProtocol::NDIStreamBus;
        screen.stream_endpoint = "ndi://192.168.1.50/nullherz_ndi_out".to_string();

        assert_eq!(screen.network_protocol, state::NetworkStreamProtocol::NDIStreamBus);
        assert_eq!(screen.stream_endpoint, "ndi://192.168.1.50/nullherz_ndi_out");
    }
}
