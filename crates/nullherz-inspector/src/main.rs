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
    DjStudio,
    Mixer,
    Sampler,
    Mastering,
    Broadcast,
    Topology,
}

pub struct Track {
    pub title: String,
    pub artist: String,
    pub bpm: f32,
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
        visuals.window_rounding = 0.0.into();

        // Custom high-end "Precision Studio" colors
        let bg_deep = egui::Color32::from_rgb(12, 12, 14);
        let accent_cyan = egui::Color32::from_rgb(0, 200, 255);
        let stroke_dim = egui::Color32::from_gray(25);

        visuals.widgets.noninteractive.bg_fill = bg_deep;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, stroke_dim);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(18, 18, 20);
        visuals.widgets.inactive.rounding = 0.0.into();
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(30, 30, 35);
        visuals.widgets.hovered.rounding = 0.0.into();
        visuals.widgets.active.bg_fill = accent_cyan;
        visuals.widgets.active.rounding = 0.0.into();

        visuals.selection.bg_fill = accent_cyan.linear_multiply(0.4);
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
            active_view: View::DjStudio,
            channel_gains: [0.8; 4],
            master_gain: 1.0,
            sample_pool: vec!["kick.wav".into(), "snare.wav".into(), "hihat.wav".into()],
            is_streaming: false,
        }
    }

    fn render_dj_studio(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        let total_w = ui.available_width();
        let main_w = total_w * 0.75;
        let lib_w = total_w * 0.25 - 4.0;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);

            // --- MAIN MIXING AREA ---
            ui.vertical(|ui| {
                ui.set_width(main_w);
                for i in 0..4 {
                    egui::Frame::canvas(ui.style())
                        .fill(egui::Color32::from_rgb(12, 12, 14))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(25)))
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.set_height(100.0);

                                // ID & Actions
                                ui.vertical(|ui| {
                                    ui.set_width(40.0);
                                    ui.label(egui::RichText::new(format!("{:02}", i+1)).color(egui::Color32::from_gray(100)).strong());
                                    ui.add_space(4.0);
                                    if ui.add(egui::Button::new("C").min_size(egui::vec2(24.0, 24.0))).clicked() {}
                                });

                                // Waveform
                                let w_width = ui.available_width() - 160.0;
                                let (w_rect, _) = ui.allocate_exact_size(egui::vec2(w_width, 80.0), egui::Sense::hover());
                                ui.painter().rect_filled(w_rect, 0.0, egui::Color32::from_rgb(5, 5, 5));

                                // Glowing wave
                                let points: Vec<egui::Pos2> = (0..w_width as i32).step_by(2).map(|x| {
                                    let phase = x as f32 * 0.08 + (ui.input(|i| i.time) * 4.0) as f32;
                                    let amp = 25.0 * ((phase * 0.005).sin().abs() + 0.3);
                                    let y = w_rect.center().y + (phase.sin() * amp);
                                    egui::pos2(w_rect.min.x + x as f32, y)
                                }).collect();
                                ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.5, egui::Color32::from_rgb(0, 220, 255))));
                                ui.painter().line(vec![egui::pos2(w_rect.min.x, w_rect.center().y), egui::pos2(w_rect.max.x, w_rect.center().y)], egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(0, 200, 255, 40)));

                                ui.add_space(10.0);

                                // EQ (Compact Sliders)
                                ui.vertical(|ui| {
                                    ui.set_width(20.0);
                                    for _ in 0..3 {
                                        ui.add(egui::Slider::new(&mut 0.0, -24.0..=6.0).show_value(false).vertical());
                                    }
                                });

                                // Fader & Integrated Meter
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    if ui.add(egui::Slider::new(&mut self.channel_gains[i], 0.0..=1.2).vertical().show_value(false)).changed() {
                                        let _ = self.command_sender.send(control_plane::Command::SetParam {
                                            target_id: (i as u64 * 3 + 2),
                                            param_id: 0,
                                            value: self.channel_gains[i],
                                            ramp_duration_samples: 128,
                                        });
                                    }

                                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*3 + 2].min(1.2));
                                    let (m_rect, _) = ui.allocate_exact_size(egui::vec2(5.0, 100.0), egui::Sense::hover());
                                    ui.painter().rect_filled(m_rect, 0.0, egui::Color32::from_gray(20));
                                    let m_h = (peak * 100.0).min(100.0);
                                    let m_p_rect = egui::Rect::from_min_size(m_rect.max - egui::vec2(5.0, m_h), egui::vec2(5.0, m_h));
                                    ui.painter().rect_filled(m_p_rect, 0.0, egui::Color32::from_rgb(0, 255, 180));
                                });
                            });
                        });
                    ui.add_space(2.0);
                }

                // Minimal Master Dashboard
                egui::Frame::none().fill(egui::Color32::from_gray(5)).inner_margin(8.0).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.set_width(main_w);
                        ui.label(egui::RichText::new("MST").small().strong());
                        ui.add(egui::Slider::new(&mut self.master_gain, 0.0..=1.5).show_value(false));

                        let m_peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[12].min(1.2));
                        let (m_rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 8.0), egui::Sense::hover());
                        ui.painter().rect_filled(m_rect, 0.0, egui::Color32::from_gray(20));
                        let m_w_val = (m_peak * 150.0).min(150.0);
                        ui.painter().rect_filled(egui::Rect::from_min_size(m_rect.min, egui::vec2(m_w_val, 8.0)), 0.0, egui::Color32::from_rgb(0, 150, 255));

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("128.0 BPM").small().color(egui::Color32::from_gray(100)));
                        });
                    });
                });
            });

            // --- TRACK LIBRARY ---
            ui.vertical(|ui| {
                ui.set_width(lib_w);
                egui::Frame::none().fill(egui::Color32::from_gray(8)).inner_margin(8.0).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("LIBRARY").color(egui::Color32::from_gray(150)).small().strong());
                        ui.add_space(4.0);
                        ui.text_edit_singleline(&mut "".to_string());
                        ui.add_space(8.0);

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let tracks = [
                                ("Deep Techno", "nullherz", 126.0),
                                ("Ambient Flow", "dsp_king", 90.0),
                                ("Glitch Hop", "rust_ace", 140.0),
                                ("Acid Bass", "tb_303", 128.0),
                                ("Minimal House", "logic_error", 124.0),
                            ];
                            for (title, artist, bpm) in tracks {
                                let (rect, res) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
                                if res.hovered() { ui.painter().rect_filled(rect, 0.0, egui::Color32::from_gray(20)); }

                                ui.child_ui(rect, egui::Layout::left_to_right(egui::Align::Center)).horizontal(|ui| {
                                    ui.add_space(4.0);
                                    ui.label(egui::RichText::new(format!("{} - {}", title, artist)).small());
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.add_space(4.0);
                                        ui.label(egui::RichText::new(format!("{:.0}", bpm)).color(egui::Color32::from_gray(80)).small());
                                    });
                                });
                            }
                        });
                    });
                });
            });
        });
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
                                let mut _active = false;
                                let color = if j % 4 == 0 { egui::Color32::from_gray(60) } else { egui::Color32::from_gray(40) };
                                let response = ui.add(egui::Button::new("").min_size(egui::vec2(18.0, 18.0)).fill(color));
                                if response.clicked() { _active = !_active; }
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

        egui::TopBottomPanel::top("nav").frame(egui::Frame::none().fill(egui::Color32::from_gray(5)).inner_margin(8.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(20.0, 0.0);
                for (view, label) in [
                    (View::DjStudio, "STUDIO"),
                    (View::Mixer, "MIXER"),
                    (View::Sampler, "SAMPLER"),
                    (View::Mastering, "MASTERING"),
                    (View::Broadcast, "BROADCAST"),
                    (View::Topology, "TOPOLOGY"),
                ] {
                    let text = egui::RichText::new(label).small().strong();
                    ui.selectable_value(&mut self.active_view, view, text);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("nullherz").color(egui::Color32::from_rgb(0, 200, 255)).strong());
                    ui.separator();
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_view {
                View::DjStudio => self.render_dj_studio(ui, &telemetry),
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
