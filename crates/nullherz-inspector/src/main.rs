use serde::{Deserialize};
use std::env;
use std::fs;
use std::net::TcpStream;
use tungstenite::{connect, stream::MaybeTlsStream, WebSocket};
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

// Graphical UI using egui
pub struct InspectorApp {
    graph: GraphJson,
    socket: Option<WebSocket<MaybeTlsStream<TcpStream>>>,
    last_telemetry: Option<Telemetry>,
}

impl InspectorApp {
    pub fn new(graph: GraphJson) -> Self {
        let socket = match connect("ws://127.0.0.1:8080") {
            Ok((s, _)) => {
                match s.get_ref() {
                    MaybeTlsStream::Plain(stream) => { stream.set_nonblocking(true).unwrap(); }
                    _ => {}
                }
                Some(s)
            }
            Err(_) => None,
        };
        Self { graph, socket, last_telemetry: None }
    }
}

impl eframe::App for InspectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll for new telemetry
        if let Some(ref mut socket) = self.socket {
            match socket.read() {
                Ok(msg) => {
                    if let tungstenite::Message::Text(text) = msg {
                        if let Ok(tel) = serde_json::from_str::<Telemetry>(&text) {
                            self.last_telemetry = Some(tel);
                        }
                    }
                }
                Err(_) => {}
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("nullherz Topology Inspector");

            if self.socket.is_none() {
                ui.colored_label(egui::Color32::RED, "Not connected to bridge");
            }

            for (i, node) in self.graph.nodes.iter().enumerate() {
                let load = self.last_telemetry.as_ref().map(|t| t.node_load_ns[i]).unwrap_or(0);
                let avg_load = self.last_telemetry.as_ref().map(|t| t.node_avg_load_ns[i]).unwrap_or(0);
                let intensity = (avg_load as f32 / 100000.0).min(1.0); // 100us as max intensity
                let color = egui::Color32::from_rgb(
                    (intensity * 255.0) as u8,
                    ((1.0 - intensity) * 255.0) as u8,
                    0
                );

                ui.horizontal(|ui| {
                    ui.label(format!("Node {}: In {:?} Out {:?}", i, node.inputs, node.outputs));
                    ui.colored_label(color, format!(" (cur: {} ns, avg: {} ns)", load, avg_load));
                    if avg_load > 50000 { // 50us bottleneck
                        ui.colored_label(egui::Color32::RED, " [BOTTLENECK]");
                    }

                    let suggestion = self.last_telemetry.as_ref().map(|t| t.optimization_suggestions[i]).unwrap_or(0);
                    match suggestion {
                        1 => { ui.colored_label(egui::Color32::YELLOW, " [SUGGEST: PARALLELIZE]"); }
                        2 => { ui.colored_label(egui::Color32::LIGHT_BLUE, " [SUGGEST: MERGE]"); }
                        _ => {}
                    }
                });
            }

            ui.separator();
            ui.heading("Buffer Levels");
            egui::ScrollArea::vertical().show(ui, |ui| {
                for i in 0..64 {
                    let level = self.last_telemetry.as_ref().map(|t| t.buffer_levels[i]).unwrap_or(0.0);
                    ui.horizontal(|ui| {
                        ui.label(format!("Buf {}: ", i));
                        ui.add(egui::ProgressBar::new(level).text(format!("{:.2}", level)));
                    });
                }
            });
        });

        ctx.request_repaint();
    }
}
