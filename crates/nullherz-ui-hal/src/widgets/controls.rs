use egui::{Align2, Color32, FontId, Rect, Response, Sense, Stroke, Ui, Vec2, vec2};

/// One shared horizontal segmented control / tab bar widget utilizing the Theme tokens.
pub fn render_segmented_control<T: PartialEq + Clone>(
    ui: &mut Ui,
    theme: &crate::Theme,
    current_value: &mut T,
    options: &[(T, &str)],
) -> Response {
    let height = 28.0;
    let item_padding = 12.0;
    let font_id = FontId::proportional(11.0);

    // Calculate total width needed
    let mut total_width = 0.0;
    let mut widths = Vec::with_capacity(options.len());
    for (_, label) in options {
        let label_w = ui.painter().layout_no_wrap(label.to_string(), font_id.clone(), Color32::WHITE).size().x;
        let item_w = label_w + item_padding * 2.0;
        widths.push(item_w);
        total_width += item_w;
    }

    let (rect, response) = ui.allocate_exact_size(Vec2::new(total_width, height), Sense::click());

    if ui.is_rect_visible(rect) {
        // Draw Rounded Pill Container Box
        ui.painter().rect(
            rect,
            theme.radius_md,
            theme.bg_inset,
            theme.border_stroke,
        );

        let mut current_x = rect.min.x;
        for (i, (val, label)) in options.iter().enumerate() {
            let item_w = widths[i];
            let segment_rect = Rect::from_min_max(
                egui::pos2(current_x, rect.min.y),
                egui::pos2(current_x + item_w, rect.max.y),
            );

            let is_selected = *current_value == *val;
            let segment_id = response.id.with(i);
            let segment_response = ui.interact(segment_rect, segment_id, Sense::click());

            if segment_response.clicked() {
                *current_value = val.clone();
                ui.ctx().request_repaint();
            }

            // Render Segment BG
            if is_selected {
                ui.painter().rect_filled(
                    segment_rect.shrink(1.5),
                    theme.radius_md,
                    theme.accent.linear_multiply(0.15),
                );
                ui.painter().rect_stroke(
                    segment_rect.shrink(1.5),
                    theme.radius_md,
                    Stroke::new(1.0, theme.accent.linear_multiply(0.4)),
                );
            } else if segment_response.hovered() {
                ui.painter().rect_filled(
                    segment_rect.shrink(1.5),
                    theme.radius_md,
                    theme.bg_med.linear_multiply(0.3),
                );
            }

            // Render Segment Text
            let text_color = if is_selected {
                theme.accent
            } else if segment_response.hovered() {
                theme.text_primary
            } else {
                theme.text_secondary
            };

            ui.painter().text(
                segment_rect.center(),
                Align2::CENTER_CENTER,
                label,
                font_id.clone(),
                text_color,
            );

            current_x += item_w;
        }
    }

    response
}

/// One shared vertical segmented control / list navigation widget utilizing the Theme tokens.
pub fn render_segmented_control_vertical<T: PartialEq + Clone>(
    ui: &mut Ui,
    theme: &crate::Theme,
    current_value: &mut T,
    options: &[(T, &str)],
    width: f32,
) -> Response {
    let item_height = 36.0;
    let total_height = item_height * options.len() as f32;
    let font_id = FontId::proportional(12.0);

    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, total_height), Sense::click());

    if ui.is_rect_visible(rect) {
        // Draw Background Pill container
        ui.painter().rect(
            rect,
            theme.radius_md,
            theme.bg_inset,
            theme.border_stroke,
        );

        for (i, (val, label)) in options.iter().enumerate() {
            let item_y = rect.min.y + i as f32 * item_height;
            let segment_rect = Rect::from_min_max(
                egui::pos2(rect.min.x, item_y),
                egui::pos2(rect.max.x, item_y + item_height),
            );

            let is_selected = *current_value == *val;
            let segment_id = response.id.with(i);
            let segment_response = ui.interact(segment_rect, segment_id, Sense::click());

            if segment_response.clicked() {
                *current_value = val.clone();
                ui.ctx().request_repaint();
            }

            // Background highlight
            if is_selected {
                ui.painter().rect_filled(
                    segment_rect.shrink(1.5),
                    theme.radius_md,
                    theme.accent.linear_multiply(0.15),
                );
                // Subtle accent border
                ui.painter().rect_stroke(
                    segment_rect.shrink(1.5),
                    theme.radius_md,
                    Stroke::new(1.0, theme.accent.linear_multiply(0.4)),
                );
                // Active Left accent indicator line (similar to sidebar nav but smaller)
                let indicator_rect = Rect::from_min_max(
                    segment_rect.left_top() + vec2(4.0, 6.0),
                    segment_rect.left_bottom() + vec2(7.0, -6.0),
                );
                ui.painter().rect_filled(indicator_rect, 1.5, theme.accent);
            } else if segment_response.hovered() {
                ui.painter().rect_filled(
                    segment_rect.shrink(1.5),
                    theme.radius_md,
                    theme.bg_med.linear_multiply(0.3),
                );
            }

            // Render label text with alignment adjusted for indicator padding
            let text_color = if is_selected {
                theme.accent
            } else if segment_response.hovered() {
                theme.text_primary
            } else {
                theme.text_secondary
            };

            let text_pos = segment_rect.left_center() + vec2(14.0, 0.0);
            ui.painter().text(
                text_pos,
                Align2::LEFT_CENTER,
                label,
                font_id.clone(),
                text_color,
            );
        }
    }

    response
}
