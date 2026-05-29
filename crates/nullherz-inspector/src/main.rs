use serde::{Deserialize};
use std::env;
use std::fs;

#[derive(Deserialize, Debug)]
struct NodeJson {
    inputs: Vec<usize>,
    outputs: Vec<usize>,
}

#[derive(Deserialize, Debug)]
struct GraphJson {
    nodes: Vec<NodeJson>,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: nullherz-inspector <graph.json>");
        return;
    }

    let content = fs::read_to_string(&args[1]).expect("Failed to read file");
    let graph: GraphJson = serde_json::from_str(&content).expect("Failed to parse JSON");

    println!("nullherz Topology Inspector");
    println!("===========================");
    render_ascii(&graph);
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
}

impl InspectorApp {
    pub fn new(graph: GraphJson) -> Self { Self { graph } }
}

impl eframe::App for InspectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("nullherz Topology Inspector");
            for (i, node) in self.graph.nodes.iter().enumerate() {
                ui.label(format!("Node {}: In {:?} Out {:?}", i, node.inputs, node.outputs));
            }
        });
    }
}
