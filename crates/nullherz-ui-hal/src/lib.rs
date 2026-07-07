pub mod widgets;
pub mod render;

#[derive(Clone, Copy)]
pub struct Theme {
    pub accent: egui::Color32,
    pub bg_dark: egui::Color32,
    pub bg_med: egui::Color32,
    pub text_primary: egui::Color32,
    pub socket_color: egui::Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: egui::Color32::from_rgb(0, 255, 200),
            bg_dark: egui::Color32::from_rgb(10, 10, 12),
            bg_med: egui::Color32::from_rgb(30, 30, 30),
            text_primary: egui::Color32::WHITE,
            socket_color: egui::Color32::from_gray(80),
        }
    }
}
