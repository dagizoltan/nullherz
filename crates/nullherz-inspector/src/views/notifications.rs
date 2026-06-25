use egui::{Color32, RichText, Ui, ScrollArea};
use crate::InspectorApp;

pub fn render(_app: &InspectorApp, ui: &mut Ui) {
    ScrollArea::vertical().show(ui, |ui| {
        ui.vertical(|ui| {
            ui.heading("Notifications");
            ui.add_space(20.0);

            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(RichText::new("No notifications").color(Color32::from_gray(60)).italics());
                ui.add_space(10.0);
                ui.label(RichText::new("System alerts and user messages will appear here.").small().color(Color32::from_gray(40)));
            });
        });
    });
}
