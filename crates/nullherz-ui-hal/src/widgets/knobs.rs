use egui::{Color32, Rect, Response, Sense, Stroke, Ui, Vec2, Align2, FontId, lerp};

/// Height reserved under the dial for its caption.
const LABEL_BAND: f32 = 12.0;
/// Gap between the dial and its caption.
const LABEL_GAP: f32 = 2.0;

pub fn render_knob(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, label: &str, accent_color: Color32) -> Response {
    render_knob_sized(ui, value, range, label, accent_color, 36.0)
}

/// A rotary control with an explicit dial diameter.
///
/// Channel strips need to fit six of these plus a fader and a meter into one
/// column, so the caller has to be able to choose the size rather than take a
/// fixed 36 px.
pub fn render_knob_sized(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    label: &str,
    accent_color: Color32,
    knob_size: f32,
) -> Response {
    let label_height = if label.is_empty() { 0.0 } else { LABEL_BAND };
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
        // Fix geometry bug: compute the knob circle's center and radius from the knob-only sub-rect,
        // completely decoupled from the label's 20px allocated space below it.
        let knob_rect = Rect::from_min_size(rect.min, Vec2::splat(knob_size));
        let center = knob_rect.center();
        let radius = knob_size / 2.0;

        // The dial body is drawn directly rather than cached.
        //
        // It used to memoise these five shapes under the widget id — but the
        // shapes hold ABSOLUTE coordinates baked in at first paint, while the
        // widget id does not change when the widget moves. Any relayout (a
        // resize, a scroll, a column count change) left the rim and face
        // painting at the position the knob used to occupy. Five circles per
        // knob is not worth a correctness hazard on a surface whose whole job
        // is to be rearranged.
        let inner_radius = radius * 0.85;
        let p = ui.painter();
        p.circle_filled(center + Vec2::new(0.0, 2.0), radius, Color32::from_black_alpha(80));
        p.circle_filled(center, radius, Color32::from_gray(25));
        p.circle_stroke(center, radius, Stroke::new(1.0, Color32::from_gray(50)));
        p.circle_filled(center, inner_radius, Color32::from_gray(35));
        p.circle_stroke(center, inner_radius, Stroke::new(0.5, Color32::from_gray(80)));

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
            // Draw the caption INSIDE the space allocated for it.
            //
            // This used to anchor at `rect.center_bottom() + 4` — the bottom of
            // the full allocation, plus four more pixels — and then lay the text
            // out downward from there. So the widget reserved a caption band and
            // then painted the caption entirely below it, on top of whatever
            // came next. In a channel strip that is the next knob, which is why
            // the dials appeared to cover their own labels.
            let knob_bottom = knob_rect.max.y;
            let anchor = egui::pos2(knob_rect.center().x, knob_bottom + LABEL_GAP);
            ui.painter().text(
                anchor,
                Align2::CENTER_TOP,
                label,
                FontId::proportional(8.5),
                Color32::from_gray(200)
            );
        }
    }

    response
}
