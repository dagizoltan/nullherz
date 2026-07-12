pub mod widgets;
pub mod render;

#[derive(Clone, Copy)]
pub struct Theme {
    pub accent: egui::Color32,
    pub bg_dark: egui::Color32,
    pub bg_med: egui::Color32,
    pub text_primary: egui::Color32,
    pub socket_color: egui::Color32,
    // --- Centralized Design Tokens (v2 Quality & Polish) ---
    pub bg_inset: egui::Color32,
    pub bg_surface: egui::Color32,
    pub text_secondary: egui::Color32,
    pub radius_md: f32,
    pub space_md: f32,
    pub border: egui::Stroke,
    pub track_colors: [egui::Color32; 16],
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: egui::Color32::from_rgb(0, 255, 200),
            bg_dark: egui::Color32::from_rgb(10, 10, 12),
            bg_med: egui::Color32::from_rgb(30, 30, 30),
            text_primary: egui::Color32::WHITE,
            socket_color: egui::Color32::from_gray(80),
            bg_inset: egui::Color32::from_rgb(15, 15, 18),
            bg_surface: egui::Color32::from_rgb(22, 22, 26),
            text_secondary: egui::Color32::from_gray(170),
            radius_md: 6.0,
            space_md: 12.0,
            border: egui::Stroke::new(1.0, egui::Color32::from_gray(40)),
            track_colors: [
                egui::Color32::from_rgb(0, 255, 200),   // Deck A Turquoise
                egui::Color32::from_rgb(0, 150, 255),   // Deck B Blue
                egui::Color32::from_rgb(255, 100, 0),   // Deck C Orange
                egui::Color32::from_rgb(255, 0, 100),   // Deck D Red
                egui::Color32::from_rgb(180, 100, 255), // Purple
                egui::Color32::from_rgb(255, 200, 0),   // Yellow
                egui::Color32::from_rgb(0, 200, 120),   // Green
                egui::Color32::from_rgb(230, 50, 230),  // Pink
                egui::Color32::from_rgb(0, 220, 220),   // Teal
                egui::Color32::from_rgb(220, 100, 100), // Salmon
                egui::Color32::from_rgb(120, 180, 120), // Sage
                egui::Color32::from_rgb(200, 140, 60),  // Bronze
                egui::Color32::from_rgb(140, 180, 220), // Ice Blue
                egui::Color32::from_rgb(220, 180, 220), // Lavender
                egui::Color32::from_rgb(180, 140, 100), // Clay
                egui::Color32::from_rgb(100, 200, 180), // Seafoam
            ],
        }
    }
}
