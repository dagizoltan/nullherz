use egui::{Ui, Color32, RichText, Sense};
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
        ui.label(RichText::new("REAL-TIME NODE GRAPH (Spatial View)").color(Color32::from_gray(100)));
        ui.add_space(10.0);

        let (canvas_rect, _response) = ui.allocate_at_least(egui::vec2(ui.available_width(), 400.0), Sense::hover());
        ui.painter().rect_filled(canvas_rect, 4.0, Color32::from_gray(30));

        for (idx, node) in app.graph.nodes.iter_mut().enumerate() {
            let node_id = ui.make_persistent_id(format!("node_spatial_{}", idx));

            // Hardened: Grid-based initial layout if coordinates are zero
            if node.x == 0.0 && node.y == 0.0 {
                node.x = 50.0 + (idx % 4) as f32 * 200.0;
                node.y = 50.0 + (idx / 4) as f32 * 120.0;
            }

            let node_pos = canvas_rect.min + egui::vec2(node.x, node.y);
            let node_size = egui::vec2(160.0, 80.0);
            let node_rect = egui::Rect::from_min_size(node_pos, node_size);

            let node_resp = ui.interact(node_rect, node_id, Sense::drag());
            if node_resp.dragged() {
                node.x += node_resp.drag_delta().x;
                node.y += node_resp.drag_delta().y;
                let _ = app.command_sender.send(Command::Topology(TopologyCommand::SetNodePosition {
                    node_idx: idx as u32,
                    x: node.x,
                    y: node.y,
                }));
            }

            // Draw Node Card
            ui.painter().rect_filled(node_rect, 4.0, Color32::from_gray(50));
            ui.painter().rect_stroke(node_rect, 4.0, egui::Stroke::new(1.0, Color32::from_gray(80)));

            // Node Header
            let header_rect = egui::Rect::from_min_size(node_pos, egui::vec2(node_size.x, 20.0));
            ui.painter().rect_filled(header_rect, 4.0, Color32::from_gray(40));
            ui.painter().text(header_rect.center(), egui::Align2::CENTER_CENTER, &node.name, egui::FontId::proportional(12.0), Color32::WHITE);

            // Sockets
            ui.allocate_ui_at_rect(node_rect, |ui| {
                ui.add_space(25.0);
                ui.horizontal(|ui| {
                    // Inputs
                    ui.vertical(|ui| {
                        for in_idx in 0..node.inputs.len() {
                            let is_occupied = app.graph.edges.iter().any(|e| e.to == idx as u32 && e.input_idx == in_idx as u32);
                            let btn_resp = ui.button(RichText::new(format!("IN {}", in_idx)).color(if is_occupied { Color32::from_rgb(0, 255, 200) } else { Color32::GRAY }).small());
                            socket_positions.insert((idx as u32, false, in_idx as u32), btn_resp.rect.center());

                            if btn_resp.clicked() || (ui.input(|i| i.pointer.any_released()) && btn_resp.hovered()) {
                                if let Some((src_node, src_out)) = app.active_connection_source {
                                    let buffer_idx = app.graph.edges.iter()
                                        .find(|e| e.from == src_node && e.output_idx == src_out)
                                        .map(|e| e.buffer_idx)
                                        .unwrap_or(src_node + 10);

                                    let _ = app.command_sender.send(Command::Topology(TopologyCommand::UpdateEdge {
                                        node_idx: idx as u32,
                                        input_idx: in_idx as u32,
                                        new_buffer_idx: buffer_idx,
                                    }));
                                    app.active_connection_source = None;
                                }
                            }
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        // Outputs
                        ui.vertical(|ui| {
                            for out_idx in 0..node.outputs.len() {
                                let btn = ui.button(RichText::new(format!("OUT {}", out_idx)).small());
                                socket_positions.insert((idx as u32, true, out_idx as u32), btn.rect.center());
                                if btn.clicked() {
                                    app.active_connection_source = Some((idx as u32, out_idx as u32));
                                }
                            }
                        });
                    });
                });
            });

            // CPU/Telemetry
            if let Some(t) = telemetry {
                 if idx < t.node_times_ns.len() {
                     let time = t.node_times_ns[idx];
                     let color = if time > 500_000 { Color32::RED } else if time > 100_000 { Color32::YELLOW } else { Color32::from_rgb(0, 255, 200) };
                     ui.painter().text(node_rect.left_bottom() + egui::vec2(5.0, -5.0), egui::Align2::LEFT_BOTTOM, format!("{} ns", time), egui::FontId::proportional(9.0), color);
                 }
            }
        }
    });

    // Draw existing cables (Based on Edge Definitions)
    let painter = ui.painter();
    for edge in &app.graph.edges {
        let start_key = (edge.from, true, 0); // Assuming primary output for now
        let end_key = (edge.to, false, edge.input_idx);

        if let (Some(&start), Some(&end)) = (socket_positions.get(&start_key), socket_positions.get(&end_key)) {
            // Hardened: Real-time cable coloring based on signal level
            let level = telemetry.as_ref().map(|t| t.peak_levels[edge.from as usize]).unwrap_or(0.0);
            let color = if level > 1.0 { Color32::from_rgb(255, 50, 50) } else if level > 0.01 { Color32::from_rgb(0, 255, 200) } else { Color32::from_gray(80) };

            // Cubic Bezier for industrial cable look
            let cp1 = start + egui::vec2(50.0, 0.0);
            let cp2 = end - egui::vec2(50.0, 0.0);
            painter.add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape {
                points: [start, cp1, cp2, end],
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: egui::Stroke::new(2.0, color),
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
