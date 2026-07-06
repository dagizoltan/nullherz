use egui::{Ui, Color32, RichText};
use crate::InspectorApp;
use audio_core::Telemetry;
use nullherz_traits::{Command, TopologyCommand};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.heading("System Topology");
    ui.add_space(10.0);

    let mut socket_positions = std::collections::HashMap::new(); // (node_idx, is_out, socket_idx) -> pos

    if let Some((src_node, src_out)) = app.active_connection_source {
        ui.label(RichText::new(format!("DRAGGING CONNECTION FROM NODE {} OUT {}", src_node, src_out)).color(Color32::YELLOW));
        if ui.button("CANCEL").clicked() { app.active_connection_source = None; }
    }

    if let Some(src_node) = app.active_node_drag {
        ui.label(RichText::new(format!("DRAGGING NODE {} (Release over remote card to migrate)", src_node)).color(Color32::LIGHT_BLUE));
        if ui.button("CANCEL DRAG").clicked() { app.active_node_drag = None; }
    }

    ui.group(|ui| {
        ui.label(RichText::new("REAL-TIME NODE GRAPH").color(Color32::from_gray(100)));
        ui.add_space(10.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (idx, node) in app.graph.nodes.iter().enumerate() {
                let _node_id = ui.make_persistent_id(format!("node_{}", idx));

                ui.horizontal(|ui| {
                    let _rect = ui.label(format!("[IDX:{}]", idx)).rect;

                    let node_label = ui.strong(&node.name);
                    if node_label.clicked() {
                        app.active_node_drag = Some(idx as u32);
                    }

                    ui.add_space(10.0);

                    // Output Sockets
                    for out_idx in 0..node.outputs.len() {
                        let btn = ui.button(format!("OUT {}", out_idx));
                        socket_positions.insert((idx as u32, true, out_idx as u32), btn.rect.center());
                        if btn.clicked() {
                            app.active_connection_source = Some((idx as u32, out_idx as u32));
                        }
                    }

                    ui.add_space(10.0);

                    // Input Sockets
                    for in_idx in 0..node.inputs.len() {
                        let btn = ui.button(format!("IN {}", in_idx));
                        socket_positions.insert((idx as u32, false, in_idx as u32), btn.rect.center());
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

                    // Remote Card / Hot-Swap target
                    let remote_addr = app.graph.node_assignments.get(&(idx as u32)).cloned().unwrap_or_else(|| "local".to_string());
                    let is_local = remote_addr == "local";
                    let _card_color = if is_local { Color32::from_gray(50) } else { Color32::from_rgb(0, 100, 200) };

                    let card_resp = ui.group(|ui| {
                        ui.label(RichText::new(&remote_addr).color(Color32::WHITE).small());
                    }).response;

                    if card_resp.clicked() {
                        if let Some(src_node) = app.active_node_drag {
                             if src_node != idx as u32 {
                                 // Emit Migration Mutation
                                 let _ = app.command_sender.send(Command::Topology(TopologyCommand::SwapProcessor {
                                     node_idx: src_node,
                                     processor_type_id: nullherz_traits::ProcessorTypeId(0), // Dummy/Marker for migration
                                 }));
                                 println!("Migrating node {} to assigned machine {}", src_node, remote_addr);
                             }
                             app.active_node_drag = None;
                        }
                    }

                    if let Some(t) = telemetry {
                         if idx < t.node_times_ns.len() {
                             let time = t.node_times_ns[idx];
                             let color = if time > 500_000 { Color32::RED } else if time > 100_000 { Color32::YELLOW } else { Color32::from_rgb(0, 255, 200) };
                             ui.label(RichText::new(format!("{} ns", time)).color(color));
                         }
                    }
                });
            }
        });
    });

    // Draw existing cables (Based on Edge Definitions)
    let painter = ui.painter();
    for edge in &app.graph.edges {
        let start_key = (edge.from, true, 0); // Assuming primary output for now
        let end_key = (edge.to, false, edge.input_idx);

        if let (Some(&start), Some(&end)) = (socket_positions.get(&start_key), socket_positions.get(&end_key)) {
            // Cubic Bezier for industrial cable look
            let cp1 = start + egui::vec2(50.0, 0.0);
            let cp2 = end - egui::vec2(50.0, 0.0);
            painter.add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape {
                points: [start, cp1, cp2, end],
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: egui::Stroke::new(2.0, Color32::from_gray(120)),
            }));
        }
    }

    // Draw active drag cable (Cubic Bezier for consistency)
    if let Some((src_node, src_out)) = app.active_connection_source {
        if let Some(&start) = socket_positions.get(&(src_node, true, src_out)) {
            if let Some(mouse_pos) = ui.input(|i| i.pointer.latest_pos()) {
                let cp1 = start + egui::vec2(50.0, 0.0);
                let cp2 = mouse_pos - egui::vec2(50.0, 0.0);
                painter.add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape {
                    points: [start, cp1, cp2, mouse_pos],
                    closed: false,
                    fill: Color32::TRANSPARENT,
                    stroke: egui::Stroke::new(2.0, Color32::YELLOW),
                }));
            }
        }
    }

    ui.add_space(20.0);
    ui.heading("Active Connections");
    ui.label("Edge connections view enhanced with visual cables.");

    ui.add_space(30.0);
    ui.separator();
    ui.heading("Sidecar Discovery");
    ui.label(RichText::new("Detected WASM and Native Sidecars in plugins/").small().color(Color32::GRAY));
    ui.add_space(10.0);

    egui::ScrollArea::vertical().id_source("sidecar_scroll").show(ui, |ui| {
        if app.discovered_sidecars.is_empty() {
            ui.label(RichText::new("No sidecars detected yet.").italics().small());
        }

        for manifest in &app.discovered_sidecars {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&manifest.name).strong());
                        ui.label(RichText::new(format!("v{} by {}", manifest.version, manifest.author)).small().color(Color32::GRAY));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                         if ui.button("HOT-LOAD").clicked() {
                             let mut name_buf = [0u8; 32];
                             let bytes = manifest.name.as_bytes();
                             let len = bytes.len().min(32);
                             name_buf[..len].copy_from_slice(&bytes[..len]);

                             let _ = app.command_sender.send(Command::Core(nullherz_traits::CoreCommand::HotLoadSidecar {
                                 name: name_buf,
                                 node_idx: 100, // Default for testing
                             }));
                         }
                    });
                });
            });
            ui.add_space(5.0);
        }
    });
}
