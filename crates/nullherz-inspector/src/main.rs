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
            "nullherz Studio",
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

#[derive(PartialEq)]
enum View {
    Mixer,
    Sampler,
    Mastering,
    Broadcast,
    Topology,
}

pub struct InspectorApp {
    graph: GraphJson,
    last_telemetry: Arc<Mutex<Option<Telemetry>>>,
    active_view: View,

    // UI State
    channel_gains: [f32; 4],
    master_gain: f32,
    sample_pool: Vec<String>,
    is_streaming: bool,
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
            active_view: View::Mixer,
            channel_gains: [0.8; 4],
            master_gain: 1.0,
            sample_pool: vec!["kick.wav".into(), "snare.wav".into(), "hihat.wav".into()],
            is_streaming: false,
        }
    }

    fn render_topology(&self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        ui.heading("Engine Topology & Performance");
        ui.add_space(10.0);
        ui.label("Cycle heatmap across 64 node slots:");

        let (rect, _response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::hover());
        let painter = ui.painter();
        let cell_w = rect.width() / 64.0;

        for i in 0..64 {
            let cell_rect = egui::Rect::from_min_size(
                rect.min + egui::vec2(i as f32 * cell_w, 0.0),
                egui::vec2(cell_w, 30.0)
            );
            let load = telemetry.as_ref().map_or(0.0, |t| (t.node_times_cycles[i] as f32 / 500000.0).min(1.0));
            let color = egui::Color32::from_rgb((load * 255.0) as u8, (255.0 * (1.0 - load)) as u8, 100);
            painter.rect_filled(cell_rect, 1.0, color);
        }

        ui.add_space(20.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, node) in self.graph.nodes.iter().enumerate() {
                let cycles = telemetry.as_ref().map(|t| t.node_times_cycles[i]).unwrap_or(0);
                ui.label(format!("Node {}: In {:?} Out {:?} ({} cycles)", i, node.inputs, node.outputs, cycles));
            }
        });
    }

    fn render_mixer(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        ui.heading("4-Channel Studio Mixer");
        ui.add_space(20.0);

        ui.horizontal(|ui| {
            for i in 0..4 {
                ui.vertical(|ui| {
                    ui.label(format!("CH {}", i + 1));

                    // Channel peak meter
                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*3 + 2].min(1.2)); // Mapping to EQ node peaks
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 150.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 0.0, egui::Color32::from_gray(30));
                    let peak_h = peak * 150.0;
                    let peak_rect = egui::Rect::from_min_size(
                        rect.max - egui::vec2(10.0, peak_h),
                        egui::vec2(10.0, peak_h)
                    );
                    ui.painter().rect_filled(peak_rect, 0.0, egui::Color32::GREEN);

                    ui.add(egui::Slider::new(&mut self.channel_gains[i], 0.0..=1.2).vertical().text(""));
                    if ui.button("Mute").clicked() { self.channel_gains[i] = 0.0; }

                    ui.add_space(10.0);
                    ui.label("FX SLOTS");
                    ui.checkbox(&mut true, "EQ");
                    ui.checkbox(&mut false, "COMP");
                });
                ui.add_space(30.0);
            }

            ui.separator();
            ui.add_space(20.0);

            ui.vertical(|ui| {
                ui.label("MASTER");

                let master_peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[12].min(1.2));
                let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 150.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 0.0, egui::Color32::from_gray(30));
                let peak_h = master_peak * 150.0;
                let peak_rect = egui::Rect::from_min_size(
                    rect.max - egui::vec2(20.0, peak_h),
                    egui::vec2(20.0, peak_h)
                );
                ui.painter().rect_filled(peak_rect, 0.0, if master_peak > 0.99 { egui::Color32::RED } else { egui::Color32::LIGHT_BLUE });

                ui.add(egui::Slider::new(&mut self.master_gain, 0.0..=1.5).vertical().text(""));
                ui.colored_label(egui::Color32::GOLD, "LIMITER");
            });
        });
    }

    fn render_sampler(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sample Deck & Music Builder");
        ui.add_space(10.0);

        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                ui.heading("Sample Pool");
                ui.add_space(5.0);
                for s in &self.sample_pool {
                    ui.horizontal(|ui| {
                        if ui.button("▶").clicked() {
                            println!("Triggering sample: {}", s);
                        }
                        ui.label(s);
                        if ui.button("🗑").clicked() { /* remove */ }
                    });
                }
                ui.add_space(10.0);
                if ui.button("➕ Load New Sample").clicked() {}
            });

            cols[1].vertical(|ui| {
                ui.heading("Trak Builder (Sequencer)");
                ui.add_space(5.0);

                for i in 0..8 {
                    ui.horizontal(|ui| {
                        ui.label(format!("Step {:02}", i + 1));
                        for _ in 0..16 {
                            let mut active = false;
                            if ui.add(egui::SelectableLabel::new(active, "")).clicked() {
                                active = !active;
                            }
                        }
                    });
                }

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let _ = ui.button("PLAY TRAK");
                    let _ = ui.button("STOP");
                    ui.label("BPM: 128");
                });
            });
        });
    }

    fn render_mastering(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        ui.heading("Global Mastering Chain");
        ui.add_space(10.0);

        ui.columns(3, |cols| {
            cols[0].vertical(|ui| {
                ui.heading("Input Stage");
                ui.label("Pre-Master VU");
                let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[12]);
                ui.add(egui::ProgressBar::new(peak).text(format!("{:.1} dB", 20.0 * peak.log10())));
            });
            cols[1].vertical(|ui| {
                ui.heading("Processing");
                ui.group(|ui| {
                    ui.checkbox(&mut true, "Linear EQ");
                    ui.checkbox(&mut true, "Multiband Comp");
                    ui.checkbox(&mut false, "Saturation");
                });
            });
            cols[2].vertical(|ui| {
                ui.heading("Final Output");
                ui.label("Post-Master VU");
                let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[12] * 0.9);
                ui.add(egui::ProgressBar::new(peak).text("FINAL"));
                ui.add_space(10.0);
                if ui.button("GENERATE MIXDOWN").clicked() {}
            });
        });
    }

    fn render_broadcast(&mut self, ui: &mut egui::Ui) {
        ui.heading("📡 Live Web Radio Stream (Broadcast)");
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            if ui.add_sized([120.0, 40.0], egui::Button::new(if self.is_streaming { "🛑 STOP" } else { "🚀 START" })
                .fill(if self.is_streaming { egui::Color32::RED } else { egui::Color32::DARK_GREEN })).clicked() {
                self.is_streaming = !self.is_streaming;
            }
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.label(format!("STATUS: {}", if self.is_streaming { "LIVE" } else { "OFFLINE" }));
                ui.label("Uptime: 00:00:00");
            });
        });

        ui.add_space(20.0);
        ui.group(|ui| {
            ui.heading("Stream Configuration");
            ui.label("Format: MP3 320kbps");
            ui.label("Server: Icecast / nullherz-edge");
            ui.horizontal(|ui| {
                ui.label("Mount Point:");
                ui.text_edit_singleline(&mut "/live".to_string());
            });
        });

        ui.add_space(20.0);
        ui.heading("Recent Listeners");
        ui.label("Total: 0");
    }
}

impl eframe::App for InspectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let telemetry = self.last_telemetry.lock().unwrap().clone();

        egui::TopBottomPanel::top("nav").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_view, View::Mixer, "🎚 Mixer");
                ui.selectable_value(&mut self.active_view, View::Sampler, "🎹 Sampler");
                ui.selectable_value(&mut self.active_view, View::Mastering, "🏆 Mastering");
                ui.selectable_value(&mut self.active_view, View::Broadcast, "📡 Broadcast");
                ui.selectable_value(&mut self.active_view, View::Topology, "🔍 Topology");
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_view {
                View::Mixer => self.render_mixer(ui, &telemetry),
                View::Sampler => self.render_sampler(ui),
                View::Topology => self.render_topology(ui, &telemetry),
                View::Mastering => self.render_mastering(ui, &telemetry),
                View::Broadcast => self.render_broadcast(ui),
            }
        });

        ctx.request_repaint();
    }
}
