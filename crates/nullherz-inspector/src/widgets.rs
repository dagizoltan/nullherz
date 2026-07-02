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
