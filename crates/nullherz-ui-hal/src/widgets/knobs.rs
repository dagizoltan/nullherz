use std::sync::Arc;
use egui::{Color32, Rect, Response, Sense, Stroke, Ui, Vec2, Align2, FontId, lerp};

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
        // Fix geometry bug: compute the knob circle's center and radius from the knob-only sub-rect,
        // completely decoupled from the label's 20px allocated space below it.
        let knob_rect = Rect::from_min_size(rect.min, Vec2::splat(knob_size));
        let center = knob_rect.center();
        let radius = knob_size / 2.0;

        // --- GEOMETRY CACHING ---
        // Cache static parts of the knob (Rim, Face, Shadow)
        // Hardened: Unique ID per widget instance to avoid geometry cache collisions
        let cache_id = response.id.with("knob_base_v3");
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
                rect.center_bottom() + Vec2::new(0.0, 4.0),
                Align2::CENTER_TOP,
                label,
                FontId::proportional(8.5),
                Color32::from_gray(200)
            );
        }
    }

    response
}
