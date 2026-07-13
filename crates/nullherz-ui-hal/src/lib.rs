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

    // Additional fields from the prompt's suggested shape & v1/v2 union
    pub bg_canvas: egui::Color32,      // app background behind panels
    pub bg_surface: egui::Color32,     // panel/card background
    pub bg_surface_raised: egui::Color32, // hover/active panel state
    pub bg_inset: egui::Color32,       // wells, input fields, track backgrounds
    pub border: egui::Color32,         // Keep as Color32 for v1 compatibility
    pub border_focus: egui::Color32,

    pub text_secondary: egui::Color32,
    pub text_disabled: egui::Color32,

    pub accent_muted: egui::Color32,   // accent at lower opacity/saturation for backgrounds

    pub success: egui::Color32,
    pub warning: egui::Color32,
    pub danger: egui::Color32,

    pub deck_colors: [egui::Color32; 4], // for multi-deck contexts (DJ console, mixer) — replaces ad hoc InspectorApp::deck_color
    pub track_colors: [egui::Color32; 16], // v2 track/deck colors

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

    // Helper Stroke for v2 compatibility
    pub border_stroke: egui::Stroke,

    // Elevation shadows
    pub shadow_sm: egui::epaint::Shadow,
    pub shadow_md: egui::epaint::Shadow,
}

impl Default for Theme {
    fn default() -> Self {
        let border_color = egui::Color32::from_gray(30);
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
            border: border_color,
            border_focus: egui::Color32::from_rgb(0, 255, 200),

            text_secondary: egui::Color32::from_gray(150),
            text_disabled: egui::Color32::from_gray(80),

            accent_muted: egui::Color32::from_rgb(0, 100, 150),

            success: egui::Color32::from_rgb(80, 220, 100),
            warning: egui::Color32::from_rgb(255, 200, 0),
            danger: egui::Color32::from_rgb(255, 50, 50),

            deck_colors: [
                egui::Color32::from_rgb(0, 255, 200),
                egui::Color32::from_rgb(0, 150, 255),
                egui::Color32::from_rgb(255, 100, 0),
                egui::Color32::from_rgb(255, 0, 100),
            ],
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

            border_stroke: egui::Stroke::new(1.0, border_color),

            shadow_sm: egui::epaint::Shadow {
                offset: egui::vec2(0.0, 1.0),
                blur: 4.0,
                spread: 0.0,
                color: egui::Color32::from_black_alpha(60),
            },
            shadow_md: egui::epaint::Shadow {
                offset: egui::vec2(0.0, 3.0),
                blur: 12.0,
                spread: 1.0,
                color: egui::Color32::from_black_alpha(100),
            },
        }
    }
}
