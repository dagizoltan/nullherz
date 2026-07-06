use egui::Ui;
use crate::InspectorApp;

pub fn render(_app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("User Account");
    ui.add_space(20.0);
    ui.label("Profile, Cloud Sync, and Subscription management.");
}
