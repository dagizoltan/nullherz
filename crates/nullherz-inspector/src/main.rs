use serde::{Deserialize};
use std::env;
use std::fs;
use audio_core::Telemetry;

#[derive(Deserialize, Debug)]
pub struct NodeJson {
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
}

#[derive(Deserialize, Debug)]
pub struct GraphJson {
    pub nodes: Vec<NodeJson>,
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: nullherz-inspector [--gui] <graph.json>");
        return Ok(());
    }

    let (gui_mode, path) = if args[1] == "--gui" {
        (true, &args[2])
    } else {
        (false, &args[1])
    };

    let content = fs::read_to_string(path).expect("Failed to read file");
    let graph: GraphJson = serde_json::from_str(&content).expect("Failed to parse JSON");

    if gui_mode {
        let native_options = eframe::NativeOptions::default();
        eframe::run_native(
            "nullherz Topology Inspector",
            native_options,
            Box::new(|_cc| {
                let app: Box<dyn eframe::App> = Box::new(InspectorApp::new(graph));
                app
            }),
        )
    } else {
        println!("nullherz Topology Inspector");
        println!("===========================");
        render_ascii(&graph);
        Ok(())
    }
}

fn render_ascii(graph: &GraphJson) {
    for (i, node) in graph.nodes.iter().enumerate() {
        let ins = node.inputs.iter().map(|idx| format!("Buf{}", idx)).collect::<Vec<_>>().join(", ");
        let outs = node.outputs.iter().map(|idx| format!("Buf{}", idx)).collect::<Vec<_>>().join(", ");

        println!("  [{}]  --> ( Node {} ) --> [{}]", ins, i, outs);
        if i < graph.nodes.len() - 1 {
            println!("             |");
        }
    }
}

// Graphical UI stub using egui
pub struct InspectorApp {
    graph: GraphJson,
    last_telemetry: Option<Telemetry>,
}

impl InspectorApp {
    pub fn new(graph: GraphJson) -> Self {
        Self {
            graph,
            last_telemetry: None,
        }
    }
}

impl eframe::App for InspectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("nullherz Topology Inspector");

            ui.add_space(20.0);
            ui.heading("Node CPU Heatmap (Micro-load)");
            {
                let (rect, _response) = ui.allocate_exact_size(egui::vec2(640.0, 40.0), egui::Sense::hover());
                let painter = ui.painter();
                for i in 0..64 {
                    let cell_rect = egui::Rect::from_min_size(
                        rect.min + egui::vec2(i as f32 * 10.0, 0.0),
                        egui::vec2(10.0, 40.0)
                    );
                    let load = if let Some(tel) = &self.last_telemetry {
                        (tel.node_times_ns[i] as f32 / 100000.0).min(1.0)
                    } else {
                        0.0
                    };
                    let color = egui::Color32::from_rgb(
                        (load * 255.0) as u8,
                        (255.0 * (1.0 - load)) as u8,
                        0
                    );
                    painter.rect_filled(cell_rect, 0.0, color);
                }
            }

            ui.add_space(20.0);
            ui.heading("Buffer Peak Levels");
            {
                let (rect, _response) = ui.allocate_exact_size(egui::vec2(640.0, 40.0), egui::Sense::hover());
                let painter = ui.painter();
                for i in 0..64 {
                    let cell_rect = egui::Rect::from_min_size(
                        rect.min + egui::vec2(i as f32 * 10.0, 0.0),
                        egui::vec2(10.0, 40.0)
                    );
                    let level = if let Some(tel) = &self.last_telemetry {
                        tel.peak_levels[i].min(1.0)
                    } else {
                        0.0
                    };
                    let color = egui::Color32::from_gray((level * 255.0) as u8);
                    painter.rect_filled(cell_rect, 0.0, color);
                }
            }

            ui.add_space(20.0);
            ui.separator();
            for (i, node) in self.graph.nodes.iter().enumerate() {
                ui.label(format!("Node {}: In {:?} Out {:?}", i, node.inputs, node.outputs));
            }
        });

        // Request constant repaints for real-time updates
        ctx.request_repaint();
    }
}
