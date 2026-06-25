use egui::{Color32, Rect, Response, Sense, Stroke, Ui, Vec2, Align2, FontId, lerp};

pub fn render_knob(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, label: &str, accent_color: Color32) -> Response {
    let knob_size = 40.0;
    let label_height = if label.is_empty() { 0.0 } else { 12.0 };
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

        // Shadow
        ui.painter().circle_filled(center + Vec2::new(0.0, 2.0), radius, Color32::from_black_alpha(80));

        // Outer Rim (Industrial Steel)
        ui.painter().circle_filled(center, radius, Color32::from_gray(25));
        ui.painter().circle_stroke(center, radius, Stroke::new(1.0, Color32::from_gray(50)));

        // Main Knob Face (Aluminum Texture Simulation)
        let inner_radius = radius * 0.85;
        ui.painter().circle_filled(center, inner_radius, Color32::from_gray(35));

        // Highlights for 3D effect
        ui.painter().circle_stroke(center, inner_radius, Stroke::new(0.5, Color32::from_gray(80)));

        // Pointer
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
                rect.center_bottom() - Vec2::new(0.0, 1.0),
                Align2::CENTER_BOTTOM,
                label,
                FontId::proportional(8.0),
                Color32::from_gray(140)
            );
        }
    }

    response
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

    // Level
    let level_h = (peak * (height / 1.2)).min(height);
    let level_rect = Rect::from_min_size(rect.max - Vec2::new(width, level_h), Vec2::new(width, level_h));

    let color = if peak > 1.0 {
        Color32::from_rgb(255, 50, 50) // Clipping
    } else if peak > 0.707 {
        Color32::from_rgb(255, 200, 0) // Warm
    } else {
        accent_color
    };

    ui.painter().rect_filled(level_rect, 0.0, color);

    // Peak Hold
    let ph_h = (peak_hold * (height / 1.2)).min(height);
    let ph_y = rect.max.y - ph_h;
    if ph_y >= rect.min.y {
        ui.painter().hline(rect.x_range(), ph_y, Stroke::new(1.0, Color32::WHITE));
    }
}

pub fn render_fader(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, accent_color: Color32) -> Response {
    let desired_size = Vec2::new(24.0, 120.0);
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
            let w = if i % 5 == 0 { 10.0 } else { 5.0 };
            ui.painter().hline(rect.center().x - w..=rect.center().x + w, y, Stroke::new(0.5, Color32::from_gray(50)));
        }

        // Handle
        let normalized = (*value - *range.start()) / (*range.end() - *range.start());
        let handle_y = rect.max.y - (normalized * rect.height());
        let handle_size = Vec2::new(20.0, 30.0);
        let handle_rect = Rect::from_center_size(egui::pos2(rect.center().x, handle_y), handle_size);

        // Handle Body
        ui.painter().rect_filled(handle_rect, 2.0, Color32::from_gray(40));
        ui.painter().rect_stroke(handle_rect, 2.0, Stroke::new(1.0, Color32::from_gray(70)));

        // Handle Indicator
        ui.painter().hline(handle_rect.x_range(), handle_rect.center().y, Stroke::new(2.0, accent_color));

        // Grip lines
        for i in [-1.0, 1.0] {
             ui.painter().hline(handle_rect.center().x - 4.0..=handle_rect.center().x + 4.0, handle_rect.center().y + i * 6.0, Stroke::new(1.0, Color32::from_gray(60)));
        }
    }

    response
}
