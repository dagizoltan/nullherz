use egui::{Ui, Frame, RichText, Sense, Vec2};
use crate::InspectorApp;

pub fn render_network(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("Distributed Sidecar Discovery");
    ui.add_space(theme.space_xs);

    // Mock/planned disclosure for remote node discovery
    ui.horizontal(|ui| {
        ui.label(RichText::new("ℹ NOTE: P2P Network Node Discovery is currently running in Mock/Simulated mode. Actual mesh/gossip node discovery is not yet fully exposed by the discovery.rs network backend.").size(9.0).color(theme.text_secondary));
    });
    ui.add_space(theme.space_xs);

    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("P2P Cloud Sync and Remote DSP Nodes").color(theme.text_secondary));
            ui.add_space(theme.space_md);

            ui.label("Remote Nodes Detected:");
            ui.add_space(theme.space_sm);

            let remote_nodes = [
                ("192.168.1.45 (Studio-PC-2)", true),
                ("192.168.1.12 (MacBook-Pro-DSP)", false),
            ];

            for (name, is_connected) in remote_nodes {
                Frame::none()
                    .fill(theme.bg_inset)
                    .rounding(theme.radius_md)
                    .stroke(theme.border_stroke)
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Status indicator dot
                            let dot_color = if is_connected { theme.success } else { theme.danger };
                            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                            ui.painter().circle_filled(dot_rect.center(), 4.0, dot_color);
                            ui.add_space(theme.space_xs);

                            ui.label(name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(if is_connected { "DISCONNECT" } else { "ATTACH" }).clicked() {
                                    println!("Toggling node connection...");
                                }
                            });
                        });
                    });
                ui.add_space(theme.space_sm);
            }
        });
}
