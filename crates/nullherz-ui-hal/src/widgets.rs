use std::sync::Arc;
use egui::{Color32, Rect, Response, Sense, Stroke, Ui, Vec2, Align2, FontId, lerp, vec2};

pub fn render_knob(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, label: &str, accent_color: Color32) -> Response {
    let knob_size = 36.0;
    let label_height = if label.is_empty() { 0.0 } else { 20.0 };
    let size = Vec2::new(knob_size, knob_size + label_height);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::drag());

    if response.dragged() {
        let old_value = *value;
        let delta = response.drag_delta().y * -0.01;
        *value = (*value + delta).clamp(*range.start(), *range.end());
        if *value != old_value {
            response.mark_changed();
        }
    }

    if ui.is_rect_visible(rect) {
        let center = rect.center();
        let radius = rect.width() / 2.0;

        // --- GEOMETRY CACHING ---
        // Cache static parts of the knob (Rim, Face, Shadow)
        let cache_id = ui.make_persistent_id("knob_base_v2");
        let shapes = ui.ctx().memory_mut(|mem| {
            mem.data.get_temp::<Arc<Vec<egui::Shape>>>(cache_id).unwrap_or_else(|| {
                 let mut s = Vec::new();
                 // Shadow
                 s.push(egui::Shape::circle_filled(center + Vec2::new(0.0, 2.0), radius, Color32::from_black_alpha(80)));
                 // Outer Rim
                 s.push(egui::Shape::circle_filled(center, radius, Color32::from_gray(25)));
                 s.push(egui::Shape::circle_stroke(center, radius, Stroke::new(1.0, Color32::from_gray(50))));
                 // Main Face
                 let inner_radius = radius * 0.85;
                 s.push(egui::Shape::circle_filled(center, inner_radius, Color32::from_gray(35)));
                 s.push(egui::Shape::circle_stroke(center, inner_radius, Stroke::new(0.5, Color32::from_gray(80))));

                 let arc = Arc::new(s);
                 mem.data.insert_temp(cache_id, arc.clone());
                 arc
            })
        });

        for shape in shapes.iter() {
            ui.painter().add(shape.clone());
        }
        let inner_radius = radius * 0.85;

        // Pointer (Dynamic)
        let normalized = (*value - *range.start()) / (*range.end() - *range.start());
        let angle = lerp((-135.0f32).to_radians()..=(135.0f32).to_radians(), normalized);
        let (sin, cos) = angle.sin_cos();

        let pointer_start = center + Vec2::new(sin, -cos) * (inner_radius * 0.3);
        let pointer_end = center + Vec2::new(sin, -cos) * (inner_radius * 0.9);

        // Pointer Glow
        let is_center = (normalized - 0.5).abs() < 0.02;
        let pointer_color = if is_center { accent_color } else { Color32::from_gray(200) };

        if is_center {
             ui.painter().line_segment([pointer_start, pointer_end], Stroke::new(4.0, accent_color.linear_multiply(0.2)));
        }

        ui.painter().line_segment([pointer_start, pointer_end], Stroke::new(3.0, Color32::BLACK));
        ui.painter().line_segment([pointer_start, pointer_end], Stroke::new(1.5, pointer_color));

        // Center Cap
        ui.painter().circle_filled(center, 3.0, Color32::from_gray(15));

        if !label.is_empty() {
            ui.painter().text(
                rect.center_bottom() + Vec2::new(0.0, 8.0),
                Align2::CENTER_TOP,
                label,
                FontId::proportional(8.5),
                Color32::from_gray(200)
            );
        }
    }

    response
}

pub fn render_spectrum_analyzer(ui: &mut Ui, spectrum: &[f32; 128], accent_color: Color32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(10, 10, 12));
    ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_gray(30)));

    let w = rect.width();
    let bin_w = w / 128.0;

    // High-performance batched mesh rendering (AnaWaves Stage 3)
    let mut mesh = egui::Mesh::default();
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

    for i in 0..128 {
        let val = spectrum[i];
        let h = (val * height * 5.0).min(height - 4.0);
        if h < 0.5 { continue; }

        let bin_rect = Rect::from_min_size(
            rect.left_bottom() + vec2(i as f32 * bin_w, -h - 2.0),
            vec2(bin_w.max(1.0), h)
        );

        // Add to mesh using standard egui API
        mesh.add_rect_with_uv(bin_rect, uv, accent_color);

        // Add highlight top line
        let top_line = Rect::from_min_size(bin_rect.min, vec2(bin_rect.width(), 1.0));
        mesh.add_rect_with_uv(top_line, uv, Color32::WHITE.linear_multiply(0.5));
    }

    ui.painter().add(egui::Shape::mesh(mesh));
}

pub fn render_goniometer(ui: &mut Ui, pts: &[f32; 128], size: f32, accent_color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(10, 10, 12));
    ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_gray(30)));

    let center = rect.center();
    let half_s = size / 2.0;

    // 45-degree axis lines
    ui.painter().line_segment([center - vec2(half_s * 0.7, half_s * 0.7), center + vec2(half_s * 0.7, half_s * 0.7)], Stroke::new(0.5, Color32::from_gray(50)));
    ui.painter().line_segment([center - vec2(-half_s * 0.7, half_s * 0.7), center + vec2(-half_s * 0.7, half_s * 0.7)], Stroke::new(0.5, Color32::from_gray(50)));

    // Optimized batched line rendering for the phase scope
    let mut points = Vec::with_capacity(64);
    for i in 0..64 {
        let l = pts[i * 2];
        let r = pts[i * 2 + 1];

        // 45-degree rotation for phase scope
        let x = (l - r) * half_s * 0.9;
        let y = -(l + r) * half_s * 0.9;
        points.push(center + vec2(x, y));
    }

    if points.len() > 1 {
        // egui's Shape::line is efficient and maps directly to vertex buffers
        ui.painter().add(egui::Shape::line(points, Stroke::new(1.2, accent_color)));
    }
}

pub fn render_vu_meter(ui: &mut Ui, peak: f32, peak_hold: f32, accent_color: Color32, height: f32) {
    let width = 8.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());

    // Background
    ui.painter().rect_filled(rect, 1.0, Color32::from_rgb(10, 10, 12));

    // Calibrated Ticks
    for db in [-48, -24, -12, -6, 0, 6] {
        let val = 10.0f32.powf(db as f32 / 20.0);
        let ty = rect.max.y - (val * (height / 1.2)).min(height);
        if ty >= rect.min.y && ty <= rect.max.y {
            ui.painter().hline(rect.x_range(), ty, Stroke::new(0.5, Color32::from_gray(60)));
        }
    }

    // High-Precision Ballistics (Asymmetrical: Fast Attack, Quadratic Decay)
    let id = ui.make_persistent_id("vu_ballistics");
    let mut smoothed_peak = ui.ctx().memory_mut(|mem| *mem.data.get_temp_mut_or_default::<f32>(id));

    let attack = 0.8;
    let decay = 0.05;
    if peak > smoothed_peak {
        smoothed_peak = smoothed_peak * (1.0 - attack) + peak * attack;
    } else {
        smoothed_peak = smoothed_peak * (1.0 - decay) + peak * decay;
    }
    ui.ctx().memory_mut(|mem| mem.data.insert_temp(id, smoothed_peak));

    // Level Rendering
    let level_h = (smoothed_peak * (height / 1.2)).min(height);
    let level_rect = Rect::from_min_size(rect.max - Vec2::new(width, level_h), Vec2::new(width, level_h));

    let color = if smoothed_peak > 1.0 {
        Color32::from_rgb(255, 50, 50) // Clipping
    } else if smoothed_peak > 0.707 {
        Color32::from_rgb(255, 200, 0) // Warm
    } else {
        accent_color
    };

    ui.painter().rect_filled(level_rect, 0.0, color);

    // Peak Hold (Digital ballistic)
    let ph_h = (peak_hold * (height / 1.2)).min(height);
    let ph_y = rect.max.y - ph_h;
    if ph_y >= rect.min.y {
        ui.painter().hline(rect.x_range(), ph_y, Stroke::new(1.0, Color32::WHITE));
    }
}

pub fn render_fader(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, accent_color: Color32, height: f32, handle_h: f32) -> Response {
    let desired_size = Vec2::new(24.0, height);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::drag());

    if response.dragged() {
        let delta = response.drag_delta().y / rect.height();
        *value = (*value - delta * (*range.end() - *range.start())).clamp(*range.start(), *range.end());
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        // Slot
        let slot_rect = Rect::from_center_size(rect.center(), Vec2::new(4.0, rect.height()));
        ui.painter().rect_filled(slot_rect, 2.0, Color32::from_gray(10));
        ui.painter().rect_stroke(slot_rect, 2.0, Stroke::new(1.0, Color32::from_gray(30)));

        // Scale Ticks
        for i in 0..=10 {
            let y = rect.min.y + (i as f32 * rect.height() / 10.0);
            let w = if i % 5 == 0 { 8.0 } else { 4.0 };
            ui.painter().hline(rect.center().x - w..=rect.center().x + w, y, Stroke::new(0.5, Color32::from_gray(50)));
        }

        // Handle
        let normalized = (*value - *range.start()) / (*range.end() - *range.start());
        let handle_y = rect.max.y - (normalized * rect.height());
        let handle_size = Vec2::new(20.0, handle_h);
        let handle_rect = Rect::from_center_size(egui::pos2(rect.center().x, handle_y), handle_size);

        // Handle Body
        ui.painter().rect_filled(handle_rect, 2.0, Color32::from_gray(40));
        ui.painter().rect_stroke(handle_rect, 2.0, Stroke::new(1.0, Color32::from_gray(70)));

        // Handle Indicator
        ui.painter().hline(handle_rect.x_range(), handle_rect.center().y, Stroke::new(2.0, accent_color));

        // Grip lines
        if handle_h > 15.0 {
            for i in [-1.0, 1.0] {
                 ui.painter().hline(handle_rect.center().x - 4.0..=handle_rect.center().x + 4.0, handle_rect.center().y + i * (handle_h / 4.0), Stroke::new(1.0, Color32::from_gray(60)));
            }
        }
    }

    response
}

pub fn render_horizontal_fader(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, accent_color: Color32, width: f32, handle_w: f32) -> Response {
    let desired_size = Vec2::new(width, 24.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::drag());

    if response.dragged() {
        let delta = response.drag_delta().x / rect.width();
        *value = (*value + delta * (*range.end() - *range.start())).clamp(*range.start(), *range.end());
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        // Slot
        let slot_rect = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.0));
        ui.painter().rect_filled(slot_rect, 2.0, Color32::from_gray(10));
        ui.painter().rect_stroke(slot_rect, 2.0, Stroke::new(1.0, Color32::from_gray(30)));

        // Scale Ticks
        for i in 0..=10 {
            let x = rect.min.x + (i as f32 * rect.width() / 10.0);
            let h = if i % 5 == 0 { 8.0 } else { 4.0 };
            ui.painter().vline(x, rect.center().y - h..=rect.center().y + h, Stroke::new(0.5, Color32::from_gray(50)));
        }

        // Handle
        let normalized = (*value - *range.start()) / (*range.end() - *range.start());
        let handle_x = rect.min.x + (normalized * rect.width());
        let handle_size = Vec2::new(handle_w, 20.0);
        let handle_rect = Rect::from_center_size(egui::pos2(handle_x, rect.center().y), handle_size);

        // Handle Body
        ui.painter().rect_filled(handle_rect, 2.0, Color32::from_gray(40));
        ui.painter().rect_stroke(handle_rect, 2.0, Stroke::new(1.0, Color32::from_gray(70)));

        // Handle Indicator
        ui.painter().vline(handle_rect.center().x, handle_rect.y_range(), Stroke::new(2.0, accent_color));

        // Grip lines
        if handle_w > 15.0 {
            for i in [-1.0, 1.0] {
                 ui.painter().vline(handle_rect.center().x + i * (handle_w / 4.0), handle_rect.center().y - 4.0..=handle_rect.center().y + 4.0, Stroke::new(1.0, Color32::from_gray(60)));
            }
        }
    }

    response
}
