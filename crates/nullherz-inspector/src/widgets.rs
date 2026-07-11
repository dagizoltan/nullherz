use egui::Color32;

pub fn render_knob(_ui: &mut egui::Ui, _value: &mut f32, _range: std::ops::RangeInclusive<f32>, _label: &str, _accent_color: Color32) -> egui::Response {
    nullherz_ui_hal::widgets::render_knob(_ui, _value, _range, _label, _accent_color)
}

pub fn render_spectrum_analyzer(ui: &mut egui::Ui, spectrum: &[f32; 128], accent_color: Color32, height: f32) {
    nullherz_ui_hal::widgets::render_spectrum_analyzer(ui, spectrum, accent_color, height)
}

pub fn render_goniometer(ui: &mut egui::Ui, pts: &[f32; 128], size: f32, accent_color: Color32) {
    nullherz_ui_hal::widgets::render_goniometer(ui, pts, size, accent_color)
}

pub fn render_vu_meter(ui: &mut egui::Ui, peak: f32, peak_hold: f32, accent_color: Color32, height: f32) {
    nullherz_ui_hal::widgets::render_vu_meter(ui, peak, peak_hold, accent_color, height)
}

pub fn render_fader(ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, accent_color: Color32, height: f32, handle_h: f32) -> egui::Response {
    nullherz_ui_hal::widgets::render_fader(ui, value, range, accent_color, height, handle_h)
}

pub fn render_horizontal_fader(ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, accent_color: Color32, width: f32, handle_w: f32) -> egui::Response {
    nullherz_ui_hal::widgets::render_horizontal_fader(ui, value, range, accent_color, width, handle_w)
}

pub fn format_duration(samples: u64, sample_rate: f32) -> String {
    if sample_rate <= 0.0 {
        return "0:00".to_string();
    }
    let total_seconds = samples as f64 / sample_rate as f64;
    let minutes = (total_seconds / 60.0).floor() as u32;
    let seconds = (total_seconds % 60.0).floor() as u32;
    format!("{}:{:02}", minutes, seconds)
}

pub fn render_time_display(ui: &mut egui::Ui, elapsed: &str, remaining: &str, accent_color: Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(elapsed).monospace().size(13.0).color(Color32::from_gray(180)));
        ui.add_space(8.0);
        ui.label(egui::RichText::new(format!("-{}", remaining)).monospace().size(13.0).color(accent_color));
    });
}

pub fn render_progress_indicator(ui: &mut egui::Ui, ratio: f32, accent_color: Color32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(width, 4.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 1.0, Color32::from_gray(30));
    let mut progress_rect = rect;
    progress_rect.set_width(width * ratio.clamp(0.0, 1.0));
    ui.painter().rect_filled(progress_rect, 1.0, accent_color);
}
