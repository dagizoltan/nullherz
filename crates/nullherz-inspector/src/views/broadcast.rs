use egui::{Ui};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("Live Broadcast Hub");
    ui.add_space(10.0);
    if ui.button(if app.is_streaming { "🛑 STOP STREAM" } else { "🚀 GO LIVE" }).clicked() { app.is_streaming = !app.is_streaming; }
    ui.label(format!("Status: {}", if app.is_streaming { "ONLINE" } else { "OFFLINE" }));
}
