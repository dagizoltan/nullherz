use egui::{Ui, Color32, RichText};
use crate::InspectorApp;
use audio_core::Telemetry;
use nullherz_traits::{Command, TopologyCommand};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.heading("System Topology");
    ui.add_space(10.0);

    if let Some((src_node, src_out)) = app.active_connection_source {
        ui.label(RichText::new(format!("DRAGGING CONNECTION FROM NODE {} OUT {}", src_node, src_out)).color(Color32::YELLOW));
        if ui.button("CANCEL").clicked() { app.active_connection_source = None; }
    }

    ui.group(|ui| {
        ui.label(RichText::new("REAL-TIME NODE GRAPH").color(Color32::from_gray(100)));
        ui.add_space(10.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (idx, node) in app.graph.nodes.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("[IDX:{}]", idx));
                    ui.strong(&node.name);

                    ui.add_space(10.0);

                    // Output Sockets
                    for out_idx in 0..node.outputs.len() {
                        if ui.button(format!("OUT {}", out_idx)).clicked() {
                            app.active_connection_source = Some((idx as u32, out_idx as u32));
                        }
                    }

                    ui.add_space(10.0);

                    // Input Sockets
                    for in_idx in 0..node.inputs.len() {
                        let btn = ui.button(format!("IN {}", in_idx));
                        if btn.clicked() {
                            if let Some((src_node, _src_out)) = app.active_connection_source {
                                // For now, we assume buffer_idx = src_node (simplification)
                                let buffer_idx = src_node + 10;
                                let _ = app.command_sender.send(Command::Topology(TopologyCommand::UpdateEdge {
                                    node_idx: idx as u32,
                                    input_idx: in_idx as u32,
                                    new_buffer_idx: buffer_idx,
                                }));
                                app.active_connection_source = None;
                            }
                        }
                    }

                    ui.add_space(20.0);

                    // Bypass Toggle
                    let mut is_bypassed = false; // Note: Current InspectorApp doesn't track bypass state locally.
                    // We'll simulate a toggle that emits the command.
                    if ui.button("BYPASS").clicked() {
                        let _ = app.command_sender.send(Command::Topology(TopologyCommand::SetBypass {
                            node_idx: idx as u32,
                            enabled: true,
                        }));
                    }

                    if let Some(t) = telemetry {
                         if idx < t.node_times_ns.len() {
                             let time = t.node_times_ns[idx];
                             let color = if time > 500_000 { Color32::RED } else if time > 100_000 { Color32::YELLOW } else { Color32::from_rgb(0, 255, 200) };
                             ui.label(RichText::new(format!("Time: {} ns", time)).color(color));
                         }
                    }
                });
            }
        });
    });

    ui.add_space(20.0);
    ui.heading("Active Connections");
    ui.label("Edge connections view coming soon.");
}
