use serde::Deserialize;
use std::env;
use std::fs;
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use audio_core::Telemetry;
use futures_util::{StreamExt, SinkExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

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
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1100.0, 700.0])
                .with_title("nullherz Studio"),
            ..Default::default()
        };
        eframe::run_native(
            "nullherz Studio",
            native_options,
            Box::new(|cc| {
                let app: Box<dyn eframe::App> = Box::new(InspectorApp::new(graph, cc));
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
    command_sender: mpsc::Sender<control_plane::Command>,
    active_view: View,

    // UI State
    channel_gains: [f32; 4],
    master_gain: f32,
    sample_pool: Vec<String>,
    is_streaming: bool,
}

impl InspectorApp {
    pub fn new(graph: GraphJson, cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.window_rounding = 8.0.into();
        visuals.widgets.active.rounding = 4.0.into();
        visuals.widgets.hovered.rounding = 4.0.into();
        visuals.widgets.inactive.rounding = 4.0.into();
        cc.egui_ctx.set_visuals(visuals);

        let last_telemetry = Arc::new(Mutex::new(None));
        let tel_clone = last_telemetry.clone();
        let (cmd_tx, cmd_rx) = mpsc::channel::<control_plane::Command>();

        // Spawn telemetry listener and command sender thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let url = "ws://127.0.0.1:9001";
                if let Ok((ws_stream, _)) = connect_async(url).await {
                    let (mut write, mut read) = ws_stream.split();

                    let sender_task = tokio::spawn(async move {
                        while let Ok(cmd) = cmd_rx.recv() {
                            let ts_cmd = control_plane::TimestampedCommand {
                                timestamp_samples: 0,
                                command: cmd,
                            };
                            if let Ok(json) = serde_json::to_string(&ts_cmd) {
                                let _ = write.send(Message::Text(json.into())).await;
                            }
                        }
                    });

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
                    sender_task.abort();
                }
            });
        });

        Self {
            graph,
            last_telemetry,
            command_sender: cmd_tx,
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
        ui.heading("Studio Mixer");
        ui.add_space(10.0);

        ui.group(|ui| {
            ui.horizontal_top(|ui| {
                for i in 0..4 {
                    ui.vertical_centered(|ui| {
                        ui.strong(format!("CH {:02}", i + 1));
                        ui.add_space(8.0);

                        // Refined Peak Meter
                        let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*3 + 2].min(1.2));
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 200.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, egui::Color32::from_gray(20));
                        let peak_h = (peak * 200.0).min(200.0);
                        let peak_rect = egui::Rect::from_min_size(
                            rect.max - egui::vec2(12.0, peak_h),
                            egui::vec2(12.0, peak_h)
                        );
                        let meter_color = if peak > 1.0 { egui::Color32::from_rgb(255, 50, 50) } else { egui::Color32::from_rgb(50, 200, 100) };
                        ui.painter().rect_filled(peak_rect, 2.0, meter_color);

                        ui.add_space(10.0);
                        if ui.add(egui::Slider::new(&mut self.channel_gains[i], 0.0..=1.2).vertical().show_value(false)).changed() {
                            let _ = self.command_sender.send(control_plane::Command::SetParam {
                                target_id: (i as u64 * 3 + 2),
                                param_id: 0,
                                value: self.channel_gains[i],
                                ramp_duration_samples: 128,
                            });
                        }
                        ui.label(format!("{:.1}", self.channel_gains[i]));

                        ui.add_space(8.0);
                        if ui.add(egui::Button::new("M").min_size(egui::vec2(24.0, 24.0))).clicked() {
                            self.channel_gains[i] = 0.0;
                            let _ = self.command_sender.send(control_plane::Command::SetParam {
                                target_id: (i as u64 * 3 + 2),
                                param_id: 0,
                                value: 0.0,
                                ramp_duration_samples: 128,
                            });
                        }

                        ui.add_space(12.0);
                        ui.group(|ui| {
                            ui.set_max_width(40.0);
                            ui.label("INS");
                            ui.checkbox(&mut true, "");
                            ui.checkbox(&mut false, "");
                        });
                    });
                    ui.add_space(24.0);
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(20.0);

                ui.vertical_centered(|ui| {
                    ui.strong("MASTER");
                    ui.add_space(8.0);

                    let master_peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[12].min(1.2));
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 200.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 3.0, egui::Color32::from_gray(15));
                    let peak_h = (master_peak * 200.0).min(200.0);
                    let peak_rect = egui::Rect::from_min_size(
                        rect.max - egui::vec2(24.0, peak_h),
                        egui::vec2(24.0, peak_h)
                    );
                    ui.painter().rect_filled(peak_rect, 3.0, if master_peak > 0.99 { egui::Color32::from_rgb(255, 100, 0) } else { egui::Color32::from_rgb(0, 150, 255) });

                    ui.add_space(10.0);
                    ui.add(egui::Slider::new(&mut self.master_gain, 0.0..=1.5).vertical().show_value(false));
                    ui.label(format!("{:.1}", self.master_gain));

                    ui.add_space(10.0);
                    ui.colored_label(egui::Color32::from_rgb(200, 150, 0), "LMT");
                });
            });
        });
    }

    fn render_sampler(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sample Deck & Sequencer");
        ui.add_space(10.0);

        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                ui.strong("SAMPLE POOL");
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.set_min_height(300.0);
                    for s in &self.sample_pool {
                        ui.horizontal(|ui| {
                            if ui.button("▶").clicked() {
                                let _ = self.command_sender.send(control_plane::Command::Play);
                            }
                            ui.label(s);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("×").clicked() {}
                            });
                        });
                    }
                    ui.add_space(10.0);
                    if ui.button("+ IMPORT WAV").clicked() {}
                });
            });

            cols[1].vertical(|ui| {
                ui.strong("SEQUENCER (TRAK BUILDER)");
                ui.add_space(8.0);

                ui.group(|ui| {
                    ui.set_min_height(300.0);
                    egui::Grid::new("sequencer_grid").spacing([4.0, 4.0]).show(ui, |ui| {
                        for i in 0..8 {
                            ui.label(format!("TRK {:02}", i + 1));
                            for j in 0..16 {
                                let mut active = false;
                                let color = if j % 4 == 0 { egui::Color32::from_gray(60) } else { egui::Color32::from_gray(40) };
                                let response = ui.add(egui::Button::new("").min_size(egui::vec2(18.0, 18.0)).fill(color));
                                if response.clicked() { active = !active; }
                            }
                            ui.end_row();
                        }
                    });
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.add_sized([100.0, 30.0], egui::Button::new("▶ PLAY").fill(egui::Color32::DARK_GREEN)).clicked() {}
                    let _ = ui.button("⏹ STOP");
                    ui.add_space(20.0);
                    ui.label("BPM:");
                    ui.add(egui::DragValue::new(&mut 128.0).speed(1.0));
                });
            });
        });
    }

    fn render_mastering(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        ui.heading("Global Mastering Chain");
        ui.add_space(10.0);

        ui.group(|ui| {
            ui.columns(3, |cols| {
                cols[0].vertical_centered(|ui| {
                    ui.strong("INPUT STAGE");
                    ui.add_space(8.0);
                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[12]);
                    ui.add(egui::ProgressBar::new(peak.min(1.0)).text("PRE-MASTER"));
                });
                cols[1].vertical_centered(|ui| {
                    ui.strong("DSP RACK");
                    ui.add_space(8.0);
                    ui.group(|ui| {
                        ui.checkbox(&mut true, "LINEAR EQ");
                        ui.checkbox(&mut true, "MULTIBAND");
                        ui.checkbox(&mut false, "SATURATION");
                    });
                });
                cols[2].vertical_centered(|ui| {
                    ui.strong("FINAL STAGE");
                    ui.add_space(8.0);
                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[12] * 0.9);
                    ui.add(egui::ProgressBar::new(peak.min(1.0)).fill(egui::Color32::GOLD).text("LUFS TARGET"));
                    ui.add_space(12.0);
                    if ui.add(egui::Button::new("📦 MIXDOWN").min_size(egui::vec2(120.0, 32.0))).clicked() {}
                });
            });
        });
    }

    fn render_broadcast(&mut self, ui: &mut egui::Ui) {
        ui.heading("📡 Studio Broadcast");
        ui.add_space(10.0);

        ui.group(|ui| {
            ui.horizontal(|ui| {
                let btn = egui::Button::new(if self.is_streaming { "🛑 OFFLINE" } else { "🚀 GO LIVE" })
                    .min_size(egui::vec2(140.0, 50.0))
                    .fill(if self.is_streaming { egui::Color32::from_rgb(180, 50, 50) } else { egui::Color32::from_rgb(50, 150, 80) });

                if ui.add(btn).clicked() {
                    self.is_streaming = !self.is_streaming;
                }
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.strong(format!("ENGINE STATUS: {}", if self.is_streaming { "STREAMING" } else { "READY" }));
                    ui.label("00:00:00.000");
                });
            });

            ui.add_space(20.0);
            ui.columns(2, |cols| {
                cols[0].vertical(|ui| {
                    ui.strong("CONFIGURATION");
                    ui.group(|ui| {
                        ui.label("Target: Icecast 2.4");
                        ui.label("Codec: OPUS @ 256kbps");
                        ui.horizontal(|ui| {
                            ui.label("Mount:");
                            ui.text_edit_singleline(&mut "/stream".to_string());
                        });
                    });
                });
                cols[1].vertical(|ui| {
                    ui.strong("AUDIENCE");
                    ui.group(|ui| {
                        ui.label("Peak Listeners: 0");
                        ui.label("Average Time: 0s");
                    });
                });
            });
        });
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
