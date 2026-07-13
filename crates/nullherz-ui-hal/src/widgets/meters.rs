use egui::{Color32, Rect, Sense, Stroke, Ui, Vec2, vec2};

/// Decoupled calculation of high-precision VU ballistic level.
/// Fast instant attack and slower geometric decay.
pub fn calculate_ballistic_vu(peak: f32, smoothed_peak: f32, decay: f32) -> f32 {
    if peak > smoothed_peak {
        peak // Instant attack
    } else {
        (smoothed_peak * (1.0 - decay)).max(0.0) // Slower decay
    }
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
    // Hardened: Unique ID per widget instance to prevent ballistics cross-talk
    let id = ui.next_auto_id();
    let mut smoothed_peak = ui.ctx().memory_mut(|mem| *mem.data.get_temp_mut_or_default::<f32>(id));

    smoothed_peak = calculate_ballistic_vu(peak, smoothed_peak, 0.02);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_ballistic_vu_attack() {
        // Attack should be instant when the input peak exceeds the current smoothed level.
        let peak = 0.8;
        let smoothed = 0.2;
        let result = calculate_ballistic_vu(peak, smoothed, 0.02);
        assert_eq!(result, 0.8);
    }

    #[test]
    fn test_calculate_ballistic_vu_decay() {
        // Decay should follow geometric falloff when the input peak is lower than the current level.
        let peak = 0.1;
        let smoothed = 0.5;
        let decay = 0.02;
        let result = calculate_ballistic_vu(peak, smoothed, decay);
        assert_eq!(result, 0.5 * (1.0 - decay));
    }

    #[test]
    fn test_calculate_ballistic_vu_clamped() {
        // Ensure that ballistic VU never drops below zero even with high decay rates.
        let peak = 0.0;
        let smoothed = 0.001;
        let decay = 0.99;
        let result = calculate_ballistic_vu(peak, smoothed, decay);
        assert!(result >= 0.0);
    }
}
