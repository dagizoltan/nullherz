pub mod widgets;
pub mod render;

#[derive(Clone, Copy)]
pub struct Theme {
    // Original 5 fields
    pub accent: egui::Color32,
    pub bg_dark: egui::Color32,
    pub bg_med: egui::Color32,
    pub text_primary: egui::Color32,
    pub socket_color: egui::Color32,

    // Additional fields from the prompt's suggested shape
    pub bg_canvas: egui::Color32,      // app background behind panels
    pub bg_surface: egui::Color32,     // panel/card background
    pub bg_surface_raised: egui::Color32, // hover/active panel state
    pub bg_inset: egui::Color32,       // wells, input fields, track backgrounds
    pub border: egui::Color32,
    pub border_focus: egui::Color32,

    pub text_secondary: egui::Color32,
    pub text_disabled: egui::Color32,

    pub accent_muted: egui::Color32,   // accent at lower opacity/saturation for backgrounds

    pub success: egui::Color32,
    pub warning: egui::Color32,
    pub danger: egui::Color32,

    pub deck_colors: [egui::Color32; 4], // for multi-deck contexts (DJ console, mixer) — replaces ad hoc InspectorApp::deck_color

    // Typography — a real scale, referenced by name not number
    pub type_caption: f32,   // 10.0
    pub type_body: f32,      // 13.0
    pub type_label: f32,     // 14.0 (UI chrome, button text)
    pub type_heading: f32,   // 18.0
    pub type_display: f32,   // 24.0
    pub type_hero: f32,      // 32.0 (used sparingly — e.g. transport time display)

    // Spacing — 4px/8px base grid
    pub space_xs: f32,   // 4.0
    pub space_sm: f32,   // 8.0
    pub space_md: f32,   // 16.0
    pub space_lg: f32,   // 24.0
    pub space_xl: f32,   // 32.0

    // Shape
    pub radius_sm: f32,  // 4.0 — buttons, chips
    pub radius_md: f32,  // 8.0 — cards, panels
    pub radius_lg: f32,  // 12.0 — modals, major surfaces
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: egui::Color32::from_rgb(0, 255, 200),
            bg_dark: egui::Color32::from_rgb(10, 10, 12),
            bg_med: egui::Color32::from_rgb(30, 30, 30),
            text_primary: egui::Color32::WHITE,
            socket_color: egui::Color32::from_gray(80),

            bg_canvas: egui::Color32::from_rgb(10, 10, 12),
            bg_surface: egui::Color32::from_rgb(18, 18, 22),
            bg_surface_raised: egui::Color32::from_rgb(30, 30, 35),
            bg_inset: egui::Color32::from_rgb(15, 15, 20),
            border: egui::Color32::from_gray(30),
            border_focus: egui::Color32::from_rgb(0, 255, 200),

            text_secondary: egui::Color32::from_gray(150),
            text_disabled: egui::Color32::from_gray(80),

            accent_muted: egui::Color32::from_rgb(0, 100, 150),

            success: egui::Color32::from_rgb(0, 255, 150),
            warning: egui::Color32::from_rgb(255, 200, 0),
            danger: egui::Color32::from_rgb(255, 50, 50),

            deck_colors: [
                egui::Color32::from_rgb(0, 255, 200),
                egui::Color32::from_rgb(0, 150, 255),
                egui::Color32::from_rgb(255, 100, 0),
                egui::Color32::from_rgb(255, 0, 100),
            ],

            type_caption: 10.0,
            type_body: 13.0,
            type_label: 14.0,
            type_heading: 18.0,
            type_display: 24.0,
            type_hero: 32.0,

            space_xs: 4.0,
            space_sm: 8.0,
            space_md: 16.0,
            space_lg: 24.0,
            space_xl: 32.0,

            radius_sm: 4.0,
            radius_md: 8.0,
            radius_lg: 12.0,
        }
    }
}
