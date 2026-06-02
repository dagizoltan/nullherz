use serde::Deserialize;
use std::env;
use std::fs;
use std::sync::{Arc, Mutex};
use audio_core::Telemetry;
use futures_util::StreamExt;
use tokio_tungstenite::connect_async;

#[derive(Deserialize, Debug, Clone)]
pub struct NodeJson {
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
}

#[derive(Deserialize, Debug, Clone)]
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

pub struct InspectorApp {
    graph: GraphJson,
    last_telemetry: Arc<Mutex<Option<Telemetry>>>,
}

impl InspectorApp {
    pub fn new(graph: GraphJson) -> Self {
        let last_telemetry = Arc::new(Mutex::new(None));
        let tel_clone = last_telemetry.clone();

        // Spawn telemetry listener thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let url = "ws://127.0.0.1:9001";
                if let Ok((ws_stream, _)) = connect_async(url).await {
                    let (_, mut read) = ws_stream.split();
                    while let Some(msg) = read.next().await {
                        if let Ok(msg) = msg {
                            if let Ok(text) = msg.to_text() {
                                if let Ok(tel) = serde_json::from_str::<Telemetry>(text) {
                                    let mut lock = tel_clone.lock().unwrap();
                                    *lock = Some(tel);
                                }
                            }
                        }
                    }
                }
            });
        });

        Self {
            graph,
            last_telemetry,
        }
    }
}

impl eframe::App for InspectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let telemetry = {
            let lock = self.last_telemetry.lock().unwrap();
            lock.clone()
        };

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("nullherz Topology Inspector");

            ui.add_space(20.0);
            ui.heading("Node CPU Heatmap (Cycle Count)");
            {
                let (rect, _response) = ui.allocate_exact_size(egui::vec2(640.0, 40.0), egui::Sense::hover());
                let painter = ui.painter();
                for i in 0..64 {
                    let cell_rect = egui::Rect::from_min_size(
                        rect.min + egui::vec2(i as f32 * 10.0, 0.0),
                        egui::vec2(10.0, 40.0)
                    );
                    let load = if let Some(tel) = &telemetry {
                        // Normalize cycle count. 100k cycles is arbitrary "high load" for visualization.
                        (tel.node_times_cycles[i] as f32 / 100000.0).min(1.0)
                    } else {
                        0.0
                    };
                    let color = egui::Color32::from_rgb(
                        (load * 255.0) as u8,
                        (255.0 * (1.0 - load)) as u8,
                        (50.0 * (1.0 - load)) as u8
                    );
                    painter.rect_filled(cell_rect, 0.0, color);
                }
            }

            ui.add_space(20.0);
            ui.heading("Buffer Peak Levels (0dBFS Heatmap)");
            {
                let (rect, _response) = ui.allocate_exact_size(egui::vec2(640.0, 40.0), egui::Sense::hover());
                let painter = ui.painter();
                for i in 0..64 {
                    let cell_rect = egui::Rect::from_min_size(
                        rect.min + egui::vec2(i as f32 * 10.0, 0.0),
                        egui::vec2(10.0, 40.0)
                    );
                    let level = if let Some(tel) = &telemetry {
                        tel.peak_levels[i].min(1.0)
                    } else {
                        0.0
                    };
                    let color = if level > 0.99 {
                        egui::Color32::RED
                    } else {
                        egui::Color32::from_gray((level * 255.0) as u8)
                    };
                    painter.rect_filled(cell_rect, 0.0, color);
                }
            }

            ui.add_space(20.0);
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, node) in self.graph.nodes.iter().enumerate() {
                    let cycles = telemetry.as_ref().map(|t| t.node_times_cycles[i]).unwrap_or(0);
                    ui.label(format!("Node {}: In {:?} Out {:?} ({} cycles)", i, node.inputs, node.outputs, cycles));
                }
            });
        });

        // Request constant repaints for real-time updates
        ctx.request_repaint();
    }
}
