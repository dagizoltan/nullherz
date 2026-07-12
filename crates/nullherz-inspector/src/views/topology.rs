use egui::{Ui, Color32, RichText, Sense, Vec2, Rounding, Stroke, Frame};
use crate::InspectorApp;
use audio_core::Telemetry;
use nullherz_traits::{Command, TopologyCommand};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    ui.heading(RichText::new("System Topology").size(theme.type_heading));
    ui.add_space(theme.space_sm);

    let mut socket_positions = std::collections::HashMap::new(); // (node_idx, is_out, socket_idx) -> pos

    if let Some((src_node, src_out)) = app.active_connection_source {
        ui.label(RichText::new(format!("DRAGGING CONNECTION FROM NODE {} OUT {}", src_node, src_out)).color(theme.warning).size(theme.type_body));
        if ui.button("CANCEL").clicked() { app.active_connection_source = None; }
    }

    if let Some(src_node) = app.active_node_drag {
        ui.label(RichText::new(format!("DRAGGING NODE {} (Release over remote card to migrate)", src_node)).color(theme.accent_muted).size(theme.type_body));
        if ui.button("CANCEL DRAG").clicked() { app.active_node_drag = None; }
    }

    // 1. Semantic Color Legend strip
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_sm)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("LEGEND:").small().strong().color(theme.text_secondary));
                ui.add_space(theme.space_xs);

                // Active Sockets
                let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, theme.accent);
                ui.label(RichText::new("Active Socket").small().color(theme.text_secondary));
                ui.add_space(theme.space_sm);

                // Idle Sockets
                let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, theme.text_disabled);
                ui.label(RichText::new("Idle Socket").small().color(theme.text_secondary));
                ui.add_space(theme.space_sm);

                // Compatible Drag Targets
                let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, theme.warning);
                ui.label(RichText::new("Compatible Target").small().color(theme.text_secondary));
                ui.add_space(theme.space_sm);

                // Bypassed Nodes
                let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, theme.danger);
                ui.label(RichText::new("Bypassed").small().color(theme.text_secondary));
                ui.add_space(theme.space_sm);

                // Host Assignments
                let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, theme.accent_muted);
                ui.label(RichText::new("Local Host").small().color(theme.text_secondary));
            });
        });

    ui.add_space(theme.space_sm);

    // 2. Real-Time Node Graph Card Container
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("REAL-TIME NODE GRAPH (Spatial View)").color(theme.text_secondary).size(theme.type_caption));
            ui.add_space(theme.space_sm);

            let (canvas_rect, _response) = ui.allocate_at_least(egui::vec2(ui.available_width(), 400.0), Sense::hover());
            ui.painter().rect_filled(canvas_rect, theme.radius_md, theme.bg_inset);

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
                ui.painter().rect_filled(node_rect, theme.radius_sm, theme.bg_surface);
                ui.painter().rect_stroke(node_rect, theme.radius_sm, theme.border_stroke);

                // Node Header
                let header_rect = egui::Rect::from_min_size(node_pos, egui::vec2(node_size.x, 20.0));
                ui.painter().rect_filled(header_rect, theme.radius_sm, theme.bg_inset);
                ui.painter().text(header_rect.left_center() + egui::vec2(5.0, 0.0), egui::Align2::LEFT_CENTER, &node.name, egui::FontId::proportional(theme.type_caption), theme.text_primary);

                // Host Assignment Badge
                let host_assignment_raw = &app.graph.node_assignments.0[idx].0;
                let host_assignment = if host_assignment_raw[0] == 0 { "local" } else {
                    std::str::from_utf8(host_assignment_raw).unwrap_or("local").trim_matches(char::from(0))
                };
                let host_rect = egui::Rect::from_center_size(header_rect.right_center() - egui::vec2(75.0, 0.0), egui::vec2(50.0, 14.0));
                let host_color = if host_assignment == "local" { theme.accent_muted } else { theme.accent };

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
                let bypass_color = if bypassed { theme.danger } else { theme.text_disabled };
                ui.painter().rect_filled(bypass_rect, 2.0, bypass_color);
                ui.painter().text(bypass_rect.center(), egui::Align2::CENTER_CENTER, "BYP", egui::FontId::monospace(9.0), theme.text_primary);

                // Industrial Sockets (Circular)
                let socket_radius = 6.0;

                // Inputs (Left Side)
                for (in_idx, _) in node.inputs.iter().enumerate() {
                    let y = node_pos.y + 35.0 + (in_idx as f32 * 18.0);
                    let socket_pos = egui::pos2(node_pos.x, y);
                    socket_positions.insert((idx as u32, false, in_idx as u32), socket_pos);

                    let is_occupied = app.graph.edges.iter().any(|e| e.to == idx as u32 && e.input_idx == in_idx as u32);
                    let color = if is_occupied { theme.accent } else { theme.socket_color };

                    let is_compatible = app.active_connection_source.is_some();
                    let stroke = if is_compatible {
                        egui::Stroke::new(2.0, theme.warning) // Gold/warning stroke for compatible inputs
                    } else {
                        egui::Stroke::new(1.0, theme.text_primary)
                    };

                    let socket_rect = egui::Rect::from_center_size(socket_pos, Vec2::splat(socket_radius * 2.5));
                    let socket_resp = ui.interact(socket_rect, node_id.with(("in", in_idx)), Sense::click());

                    ui.painter().circle_filled(socket_pos, socket_radius, color);
                    ui.painter().circle_stroke(socket_pos, socket_radius, stroke);

                    if socket_resp.hovered() {
                        ui.painter().circle_stroke(socket_pos, socket_radius + 2.0, egui::Stroke::new(1.0, theme.warning));
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

                    ui.painter().circle_filled(socket_pos, socket_radius, theme.text_disabled);
                    ui.painter().circle_stroke(socket_pos, socket_radius, egui::Stroke::new(1.0, theme.text_primary));

                    if socket_resp.clicked() {
                        app.active_connection_source = Some((idx as u32, out_idx as u32));
                    }
                }

                // CPU/Telemetry
                if let Some(t) = telemetry {
                     if idx < t.node_times_ns.len() {
                         let time = t.node_times_ns[idx];
                         let color = if time > 500_000 { theme.danger } else if time > 100_000 { theme.warning } else { theme.accent };
                         ui.painter().text(node_rect.left_bottom() + egui::vec2(5.0, -5.0), egui::Align2::LEFT_BOTTOM, format!("{} ns", time), egui::FontId::proportional(theme.type_caption), color);
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
            let base_color = if level > 1.0 { theme.danger } else if level > 0.01 { theme.accent } else { theme.text_disabled };

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
            painter.text(start + egui::vec2(10.0, -10.0), egui::Align2::LEFT_BOTTOM, format!("B{}", edge.buffer_idx), egui::FontId::monospace(theme.type_caption), theme.text_secondary);

            // Signal Flow Animation (Moving dashes)
            if level > 0.01 {
                let dash_count = 3;
                for j in 0..dash_count {
                    let t_offset = (time as f32 * 0.5 + (j as f32 / dash_count as f32)) % 1.0;
                    let p = bezier.sample(t_offset);
                    painter.circle_filled(p, 2.0, theme.text_primary.gamma_multiply(0.8));
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
                    stroke: egui::Stroke::new(2.0, theme.warning),
                }));
            }
        }
    }

    ui.add_space(theme.space_md);

    // 3. Tabular Connections List (Replacing vestigial connections section)
    ui.heading(RichText::new("Active Connections List").size(theme.type_heading));
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            if app.graph.edges.is_empty() {
                ui.label(RichText::new("No active connection paths established.").italics().color(theme.text_disabled));
            } else {
                egui::Grid::new("connections_list_grid")
                    .num_columns(4)
                    .spacing([24.0, 10.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new("SOURCE NODE").strong().color(theme.text_secondary).size(theme.type_caption));
                        ui.label(RichText::new("DESTINATION NODE").strong().color(theme.text_secondary).size(theme.type_caption));
                        ui.label(RichText::new("BUFFER ID").strong().color(theme.text_secondary).size(theme.type_caption));
                        ui.label(RichText::new("PEAK SIGNAL LEVEL").strong().color(theme.text_secondary).size(theme.type_caption));
                        ui.end_row();

                        for edge in &app.graph.edges {
                            let src_name = app.graph.nodes.get(edge.from as usize).map(|n| n.name.as_str()).unwrap_or("Unknown");
                            let dst_name = app.graph.nodes.get(edge.to as usize).map(|n| n.name.as_str()).unwrap_or("Unknown");
                            let level = telemetry.as_ref().map(|t| t.peak_levels[edge.from as usize]).unwrap_or(0.0);

                            ui.label(src_name);
                            ui.label(format!("{} (In {})", dst_name, edge.input_idx));
                            ui.label(format!("B{}", edge.buffer_idx));

                            // Visual bar/level
                            ui.horizontal(|ui| {
                                ui.add(egui::ProgressBar::new(level.min(1.0)).desired_width(60.0).fill(theme.accent).show_percentage());
                            });
                            ui.end_row();
                        }
                    });
            }
        });

    ui.add_space(theme.space_lg);
    ui.separator();
    ui.add_space(theme.space_sm);

    ui.heading(RichText::new("Sidecar Discovery").size(theme.type_heading));
    ui.label(RichText::new("Detected WASM and Native Sidecars in plugins/").size(theme.type_caption).color(theme.text_secondary));
    ui.add_space(theme.space_sm);

    // 4. Sidecars Card List with dynamic combo-box targeting
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            egui::ScrollArea::vertical().id_source("sidecar_scroll").show(ui, |ui| {
                if app.discovered_sidecars.is_empty() {
                    ui.label(RichText::new("No sidecars detected yet.").italics().size(theme.type_caption));
                }

                // Target ComboBox for Hot-Loading
                if !app.discovered_sidecars.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label("Hot-Load Target Node:");
                        egui::ComboBox::from_id_source("hotload_target_select")
                            .selected_text(app.graph.nodes.get(app.selected_hotload_node_idx).map(|n| n.name.as_str()).unwrap_or("Select Target Node"))
                            .show_ui(ui, |ui| {
                                for (n_idx, n) in app.graph.nodes.iter().enumerate() {
                                    ui.selectable_value(&mut app.selected_hotload_node_idx, n_idx, &n.name);
                                }
                            });
                    });
                    ui.add_space(theme.space_sm);
                }

                for manifest in &app.discovered_sidecars {
                    Frame::none()
                        .fill(theme.bg_inset)
                        .rounding(theme.radius_sm)
                        .stroke(theme.border_stroke)
                        .inner_margin(theme.space_sm)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&manifest.name).strong().size(theme.type_body));
                                    ui.label(RichText::new(format!("v{} by {}", manifest.version, manifest.author)).size(theme.type_caption).color(theme.text_secondary));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                     if ui.button(RichText::new("HOT-LOAD").size(theme.type_label)).clicked() {
                                         let mut name_buf = [0u8; 32];
                                         let bytes = manifest.name.as_bytes();
                                         let len = bytes.len().min(32);
                                         name_buf[..len].copy_from_slice(&bytes[..len]);

                                         let _ = app.command_sender.send(Command::Core(nullherz_traits::CoreCommand::HotLoadSidecar {
                                             name: name_buf,
                                             node_idx: app.selected_hotload_node_idx as u32, // Dynamically targeted!
                                         }));
                                     }
                                });
                            });
                        });
                    ui.add_space(theme.space_xs);
                }
            });
        });
}
