use egui::{Color32, RichText, Ui, Frame, ScrollArea, Layout, Align, Rect, Id, Sense};
use crate::InspectorApp;
use audio_core::Telemetry;

pub fn render(app: &InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.heading("Engine Topology & Performance Heatmap");
    ui.add_space(10.0);

    // PERFORMANCE HEATMAP RULER
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), Sense::hover());
    ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(10, 10, 12));

    let cell_w = rect.width() / 64.0;
    for i in 0..64 {
        let load = telemetry.as_ref().map_or(0.0, |t| t.node_times_ns[i] as f32 / 1_000_000.0).min(1.0); // 1ms scale
        let color = if load > 0.8 {
            Color32::from_rgb(255, 50, 50)
        } else if load > 0.4 {
            Color32::from_rgb(255, 150, 0)
        } else if load > 0.01 {
            Color32::from_rgb(0, 255, 150)
        } else {
            Color32::from_gray(20)
        };

        let cell_rect = Rect::from_min_size(rect.min + egui::vec2(i as f32 * cell_w, 0.0), egui::vec2(cell_w - 1.0, 32.0));
        ui.painter().rect_filled(cell_rect, 1.0, color);

        if ui.rect_contains_pointer(cell_rect) {
            egui::show_tooltip(ui.ctx(), Id::new("node_tooltip"), |ui| {
                ui.label(format!("Node {}: {:.3} ms", i, load));
            });
        }
    }
    ui.add_space(20.0);

    ScrollArea::vertical().show(ui, |ui| {
        ui.set_width(ui.available_width());

        for (i, node) in app.graph.nodes.iter().enumerate() {
            let load = telemetry.as_ref().map_or(0.0, |t| t.node_times_ns[i] as f32 / 1_000_000.0);
            let color = if load > 0.8 { Color32::RED } else { Color32::from_gray(150) };

            Frame::none().fill(Color32::from_rgb(20, 20, 24)).rounding(2.0).inner_margin(8.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(format!("NODE {:02}", i)).strong().color(color));
                    ui.add_space(20.0);

                    ui.label(RichText::new("INPUTS").small().color(Color32::from_gray(80)));
                    ui.label(format!("{:?}", node.inputs));

                    ui.label(RichText::new("OUTPUTS").small().color(Color32::from_gray(80)));
                    ui.label(format!("{:?}", node.outputs));

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(format!("{:.3} ms", load)).monospace().color(color));

                        // Load bar
                        let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(100.0, 8.0), Sense::hover());
                        ui.painter().rect_filled(bar_rect, 4.0, Color32::from_gray(30));
                        let fill_w = load.min(1.0) * 100.0;
                        ui.painter().rect_filled(Rect::from_min_size(bar_rect.min, egui::vec2(fill_w, 8.0)), 4.0, color);
                    });
                });
            });
            ui.add_space(4.0);
        }
    });
}
