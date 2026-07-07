use egui::{Ui, Color32, RichText, Sense, Vec2};
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
            ui.painter().text(header_rect.left_center() + egui::vec2(5.0, 0.0), egui::Align2::LEFT_CENTER, &node.name, egui::FontId::proportional(11.0), Color32::WHITE);

            // Host Assignment Badge
            let host_assignment = app.graph.node_assignments.get(&(idx as u32)).map(|s| s.as_str()).unwrap_or("local");
            let host_rect = egui::Rect::from_center_size(header_rect.right_center() - egui::vec2(75.0, 0.0), egui::vec2(50.0, 14.0));
            let host_color = if host_assignment == "local" { Color32::from_rgb(0, 150, 255) } else { app.theme.accent };

            let host_resp = ui.interact(host_rect, node_id.with("host_migrate"), Sense::click());
            if host_resp.clicked() {
                 let next_host = if host_assignment == "local" { "remote_1" } else { "local" };
                 let mut dest_buf = [0u8; 32];
                 let bytes = next_host.as_bytes();
                 dest_buf[..bytes.len()].copy_from_slice(bytes);
                 let _ = app.command_sender.send(Command::Topology(TopologyCommand::MigrateNode {
                     node_idx: idx as u32,
                     destination: dest_buf,
                 }));
            }
            ui.painter().rect_filled(host_rect, 2.0, host_color.gamma_multiply(0.3));
            ui.painter().text(host_rect.center(), egui::Align2::CENTER_CENTER, host_assignment.to_uppercase(), egui::FontId::monospace(8.0), host_color);

            // Bypass Toggle in header
            let bypass_rect = egui::Rect::from_center_size(header_rect.right_center() - egui::vec2(25.0, 0.0), egui::vec2(40.0, 14.0));
            let bypass_id = node_id.with("bypass");
            let bypassed = app.bypassed_nodes.contains(&(idx as u32));

            let bypass_resp = ui.interact(bypass_rect, bypass_id, Sense::click());
            if bypass_resp.clicked() {
                let new_state = !bypassed;
                if new_state {
                    app.bypassed_nodes.insert(idx as u32);
                } else {
                    app.bypassed_nodes.remove(&(idx as u32));
                }
                let _ = app.command_sender.send(Command::Topology(TopologyCommand::SetBypass {
                    node_idx: idx as u32,
                    enabled: new_state,
                }));
            }
            let bypass_color = if bypassed { Color32::from_rgb(255, 100, 0) } else { Color32::from_gray(60) };
            ui.painter().rect_filled(bypass_rect, 2.0, bypass_color);
            ui.painter().text(bypass_rect.center(), egui::Align2::CENTER_CENTER, "BYP", egui::FontId::monospace(9.0), Color32::WHITE);

            // Industrial Sockets (Circular)
            let socket_radius = 6.0;

            // Inputs (Left Side)
            for (in_idx, _) in node.inputs.iter().enumerate() {
                let y = node_pos.y + 35.0 + (in_idx as f32 * 18.0);
                let socket_pos = egui::pos2(node_pos.x, y);
                socket_positions.insert((idx as u32, false, in_idx as u32), socket_pos);

                let is_occupied = app.graph.edges.iter().any(|e| e.to == idx as u32 && e.input_idx == in_idx as u32);
                let mut color = if is_occupied { app.theme.accent } else { app.theme.socket_color };

                let is_compatible = app.active_connection_source.is_some();
                let stroke = if is_compatible {
                    egui::Stroke::new(2.0, Color32::from_rgb(255, 215, 0)) // Gold stroke for compatible inputs
                } else {
                    egui::Stroke::new(1.0, Color32::WHITE)
                };

                let socket_rect = egui::Rect::from_center_size(socket_pos, Vec2::splat(socket_radius * 2.5));
                let socket_resp = ui.interact(socket_rect, node_id.with(("in", in_idx)), Sense::click());

                ui.painter().circle_filled(socket_pos, socket_radius, color);
                ui.painter().circle_stroke(socket_pos, socket_radius, stroke);

                if socket_resp.hovered() {
                    ui.painter().circle_stroke(socket_pos, socket_radius + 2.0, egui::Stroke::new(1.0, Color32::YELLOW));
                    if ui.input(|i| i.pointer.any_released()) {
                        if let Some((src_node, src_out)) = app.active_connection_source {
                            let _ = app.command_sender.send(Command::Topology(TopologyCommand::Connect {
                                src_node_idx: src_node,
                                src_output_idx: src_out,
                                dst_node_idx: idx as u32,
                                dst_input_idx: in_idx as u32,
                            }));
                            app.active_connection_source = None;
                        }
                    }
                }

                if socket_resp.secondary_clicked() {
                    let _ = app.command_sender.send(Command::Topology(TopologyCommand::Disconnect {
                        node_idx: idx as u32,
                        input_idx: in_idx as u32,
                    }));
                }
            }

            // Outputs (Right Side)
            for (out_idx, _) in node.outputs.iter().enumerate() {
                let y = node_pos.y + 35.0 + (out_idx as f32 * 18.0);
                let socket_pos = egui::pos2(node_pos.x + node_size.x, y);
                socket_positions.insert((idx as u32, true, out_idx as u32), socket_pos);

                let socket_rect = egui::Rect::from_center_size(socket_pos, Vec2::splat(socket_radius * 2.5));
                let socket_resp = ui.interact(socket_rect, node_id.with(("out", out_idx)), Sense::click());

                ui.painter().circle_filled(socket_pos, socket_radius, Color32::from_gray(100));
                ui.painter().circle_stroke(socket_pos, socket_radius, egui::Stroke::new(1.0, Color32::WHITE));

                if socket_resp.clicked() {
                    app.active_connection_source = Some((idx as u32, out_idx as u32));
                }
            }

            // CPU/Telemetry
            if let Some(t) = telemetry {
                 if idx < t.node_times_ns.len() {
                     let time = t.node_times_ns[idx];
                     let color = if time > 500_000 { Color32::RED } else if time > 100_000 { Color32::YELLOW } else { app.theme.accent };
                     ui.painter().text(node_rect.left_bottom() + egui::vec2(5.0, -5.0), egui::Align2::LEFT_BOTTOM, format!("{} ns", time), egui::FontId::proportional(9.0), color);
                 }
            }
        }
    });

    // Draw existing cables (Based on Edge Definitions)
    let painter = ui.painter();
    let time = ui.input(|i| i.time);

    for edge in &app.graph.edges {
        let start_key = (edge.from, true, edge.output_idx);
        let end_key = (edge.to, false, edge.input_idx);

        if let (Some(&start), Some(&end)) = (socket_positions.get(&start_key), socket_positions.get(&end_key)) {
            let level = telemetry.as_ref().map(|t| t.peak_levels[edge.from as usize]).unwrap_or(0.0);
            let base_color = if level > 1.0 { Color32::from_rgb(255, 50, 50) } else if level > 0.01 { app.theme.accent } else { Color32::from_gray(80) };

            let cp1 = start + egui::vec2(60.0, 0.0);
            let cp2 = end - egui::vec2(60.0, 0.0);
            let glow = if level > 0.01 { ui.input(|i| (i.time.cos() * 0.15 + 0.85) as f32) } else { 1.0 };
            let bezier = egui::epaint::CubicBezierShape {
                points: [start, cp1, cp2, end],
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: egui::Stroke::new(2.0, base_color.gamma_multiply(glow)),
            };
            painter.add(egui::Shape::CubicBezier(bezier));

            // Buffer Index Label
            painter.text(start + egui::vec2(10.0, -10.0), egui::Align2::LEFT_BOTTOM, format!("B{}", edge.buffer_idx), egui::FontId::monospace(9.0), Color32::from_gray(150));

            // Signal Flow Animation (Moving dashes)
            if level > 0.01 {
                let dash_count = 3;
                for j in 0..dash_count {
                    let t_offset = (time as f32 * 0.5 + (j as f32 / dash_count as f32)) % 1.0;
                    let p = bezier.sample(t_offset);
                    painter.circle_filled(p, 2.0, Color32::WHITE.gamma_multiply(0.8));
                }
            }
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
