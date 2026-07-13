use egui::{Color32, Rect, Response, Sense, Stroke, Ui, Vec2};

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
