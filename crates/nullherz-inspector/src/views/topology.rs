use egui::{Ui, Color32, RichText};
use crate::InspectorApp;
use audio_core::Telemetry;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.heading("System Topology");
    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label(RichText::new("REAL-TIME NODE GRAPH").color(Color32::from_gray(100)));
        ui.add_space(10.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (idx, node) in app.graph.nodes.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("[IDX:{}]", idx));
                    ui.strong(&node.name);

                    if let Some(t) = telemetry {
                         if idx < t.node_times_ns.len() {
                             let time = t.node_times_ns[idx];
                             ui.label(format!("Time: {} ns", time));
                         }
                    }
                });
            }
        });
    });

    ui.add_space(20.0);
    ui.heading("Remote DSP Nodes");
    ui.group(|ui| {
        // In a real implementation we would fetch this from the conductor via a dedicated channel
        // For now, let's assume we have a way to see them.
        ui.label("Remote Node Discovery active on port 9000");
    });

    ui.add_space(20.0);
    ui.heading("Active Connections");
    ui.label("Edge connections view coming soon.");
}
