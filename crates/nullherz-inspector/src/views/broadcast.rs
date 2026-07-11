use egui::{Ui, Frame, Margin, Rounding, Stroke, Color32, RichText};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.heading(RichText::new("Global Broadcast").size(theme.type_heading));
    ui.add_space(theme.space_md);

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .inner_margin(Margin::same(theme.space_md))
        .stroke(Stroke::new(1.0, theme.border))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.heading(RichText::new("Live Broadcast Settings").size(theme.type_body));
                ui.add_space(theme.space_sm);

                // Status Indicator with colored dot
                ui.horizontal(|ui| {
                    let dot_color = if app.is_streaming { theme.success } else { theme.text_disabled };
                    let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot_rect.center(), 4.0, dot_color);
                    ui.label(RichText::new("STATUS:").size(theme.type_caption).strong());
                    ui.label(RichText::new(if app.is_streaming { "ONLINE" } else { "OFFLINE" })
                        .size(theme.type_caption)
                        .color(if app.is_streaming { theme.success } else { theme.text_secondary }));
                });

                ui.add_space(theme.space_sm);

                // Stream Configuration UI Details
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Stream Target:").size(theme.type_caption).color(theme.text_secondary));
                    ui.label(RichText::new("rtmp://gossip.genetic.cloud/live").size(theme.type_caption).monospace());
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Codec & Bitrate:").size(theme.type_caption).color(theme.text_secondary));
                    ui.label(RichText::new("Opus (256 kbps, Mastering Grade)").size(theme.type_caption));
                });

                ui.add_space(theme.space_md);

                // Button styled as primary action (accent fill)
                let button_text = if app.is_streaming { "🛑 STOP STREAM" } else { "🚀 GO LIVE" };
                let button_color = if app.is_streaming { theme.danger } else { theme.accent };
                let button_text_color = if app.is_streaming { theme.text_primary } else { Color32::BLACK };

                let btn = egui::Button::new(RichText::new(button_text).strong().size(theme.type_label).color(button_text_color))
                    .fill(button_color);

                if ui.add_sized([150.0, 30.0], btn).clicked() {
                    app.is_streaming = !app.is_streaming;
                }
            });
        });
}
