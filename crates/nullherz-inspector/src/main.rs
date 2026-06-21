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
        if args.len() < 3 {
            eprintln!("Error: --gui requires a <graph.json> path.");
            eprintln!("Usage: nullherz-inspector [--gui] <graph.json>");
            return Ok(());
        }
        (true, &args[2])
    } else {
        (false, &args[1])
    };

    let content = fs::read_to_string(path).expect("Failed to read file");
    let graph: GraphJson = serde_json::from_str(&content).expect("Failed to parse JSON");

    if gui_mode {
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1200.0, 800.0])
                .with_title("nullherz Precision Studio"),
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
    Studio,
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
    command_sender: mpsc::Sender<nullherz_traits::Command>,
    active_view: View,

    // UI State — all controls bound to persistent state
    channel_gains: [f32; 4],
    channel_eq_high: [f32; 4],
    channel_eq_mid: [f32; 4],
    channel_eq_low: [f32; 4],
    channel_cue: [bool; 4],
    master_gain: f32,
    sample_pool: Vec<String>,
    search_query: String,
    is_streaming: bool,

    // Mastering chain state
    mastering_eq_enabled: bool,
    mastering_comp_enabled: bool,
    mastering_limiter_enabled: bool,
}

impl InspectorApp {
    pub fn new(graph: GraphJson, cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.window_rounding = 0.0.into();

        let bg_deep = egui::Color32::from_rgb(10, 10, 12);
        let accent_cyan = egui::Color32::from_rgb(0, 220, 255);
        let stroke_dim = egui::Color32::from_gray(25);

        visuals.widgets.noninteractive.bg_fill = bg_deep;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, stroke_dim);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(18, 18, 22);
        visuals.widgets.inactive.rounding = 0.0.into();
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(30, 30, 35);
        visuals.widgets.active.bg_fill = accent_cyan;

        visuals.selection.bg_fill = accent_cyan.linear_multiply(0.3);
        cc.egui_ctx.set_visuals(visuals);

        let last_telemetry = Arc::new(Mutex::new(None));
        let tel_clone = last_telemetry.clone();
        let (cmd_tx, cmd_rx) = mpsc::channel::<nullherz_traits::Command>();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let url = "ws://127.0.0.1:9001";
                if let Ok((ws_stream, _)) = connect_async(url).await {
                    let (mut write, mut read) = ws_stream.split();

                    let sender_task = tokio::spawn(async move {
                        while let Ok(cmd) = cmd_rx.recv() {
                            let ts_cmd = nullherz_traits::TimestampedCommand {
                                timestamp_samples: 0,
                                command: cmd,
                            };
                            if let Ok(json) = serde_json::to_string(&ts_cmd) {
                                let _ = write.send(Message::Text(json.into())).await;
                            }
                        }
                    });

                    while let Some(Ok(msg)) = read.next().await {
                        if let Ok(text) = msg.to_text() {
                            let tel_res = serde_json::from_str::<Telemetry>(text);
                            if let Ok(tel) = tel_res {
                                let mut lock = tel_clone.lock().unwrap();
                                *lock = Some(tel);
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
            active_view: View::Studio,
            channel_gains: [0.8; 4],
            channel_eq_high: [0.0; 4],
            channel_eq_mid: [0.0; 4],
            channel_eq_low: [0.0; 4],
            channel_cue: [false; 4],
            master_gain: 1.0,
            sample_pool: vec!["kick.wav".into(), "snare.wav".into(), "hihat.wav".into()],
            search_query: String::new(),
            is_streaming: false,
            mastering_eq_enabled: true,
            mastering_comp_enabled: true,
            mastering_limiter_enabled: false,
        }
    }

    fn render_dj_studio(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        let total_w = ui.available_width();
        let main_w = total_w * 0.75;
        let lib_w = total_w * 0.25;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

            // --- MAIN MIXING AREA ---
            ui.vertical(|ui| {
                ui.set_width(main_w);
                for i in 0..4 {
                    let rect = ui.allocate_exact_size(egui::vec2(main_w, 150.0), egui::Sense::hover()).0;
                    ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_gray(20)));

                    ui.child_ui(rect, egui::Layout::left_to_right(egui::Align::Center)).horizontal(|ui| {
                        ui.add_space(20.0);
                        // ID & Info
                        ui.vertical(|ui| {
                            ui.set_width(60.0);
                            ui.label(egui::RichText::new(format!("{:02}", i+1)).color(egui::Color32::from_gray(100)).strong().size(20.0));
                            ui.add_space(10.0);
                            let cue_color = if self.channel_cue[i] {
                                egui::Color32::from_rgb(255, 180, 0)
                            } else {
                                egui::Color32::from_gray(80)
                            };
                            if ui.add(egui::Button::new(egui::RichText::new("CUE").color(cue_color).size(11.0))).clicked() {
                                self.channel_cue[i] = !self.channel_cue[i];
                            }
                        });

                        // Precision Waveform
                        let w_width = main_w - 300.0;
                        let (w_rect, _) = ui.allocate_exact_size(egui::vec2(w_width, 110.0), egui::Sense::hover());
                        ui.painter().rect_filled(w_rect, 0.0, egui::Color32::from_rgb(5, 5, 5));

                        let points: Vec<egui::Pos2> = (0..w_width as i32).step_by(2).map(|x| {
                            let phase = x as f32 * 0.08 + (ui.input(|i| i.time) * 4.0) as f32;
                            let amp = 30.0 * ((phase * 0.005).sin().abs() + 0.2);
                            let y = w_rect.center().y + (phase.sin() * amp);
                            egui::pos2(w_rect.min.x + x as f32, y)
                        }).collect();
                        ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 200, 255))));
                        ui.painter().hline(w_rect.x_range(), w_rect.center().y, egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(0, 200, 255, 40)));

                        ui.add_space(20.0);

                        // EQ & Gain — now bound to persistent state
                        ui.vertical_centered(|ui| {
                            ui.set_width(40.0);
                            ui.label(egui::RichText::new("H").size(9.0).color(egui::Color32::from_gray(80)));
                            ui.add(egui::Slider::new(&mut self.channel_eq_high[i], -24.0..=6.0).show_value(false).vertical());
                            ui.label(egui::RichText::new("M").size(9.0).color(egui::Color32::from_gray(80)));
                            ui.add(egui::Slider::new(&mut self.channel_eq_mid[i], -24.0..=6.0).show_value(false).vertical());
                            ui.label(egui::RichText::new("L").size(9.0).color(egui::Color32::from_gray(80)));
                            ui.add(egui::Slider::new(&mut self.channel_eq_low[i], -24.0..=6.0).show_value(false).vertical());
                        });

                        // Fader & Meter
                        ui.horizontal(|ui| {
                            ui.add_space(20.0);
                            if ui.add(egui::Slider::new(&mut self.channel_gains[i], 0.0..=1.2).vertical().show_value(false)).changed() {
                                // PD-2: Corrected target_id for GainProcessor in new 4-node-per-deck mixer
                                let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                    target_id: (i as u64 * 4 + 1),
                                    param_id: 0,
                                    value: self.channel_gains[i],
                                    ramp_duration_samples: 128,
                                });
                            }

                            let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1].min(1.2));
                            let (m_rect, _) = ui.allocate_exact_size(egui::vec2(6.0, 130.0), egui::Sense::hover());
                            ui.painter().rect_filled(m_rect, 0.0, egui::Color32::from_gray(20));
                            let m_h = (peak * 130.0).min(130.0);
                            let m_p_rect = egui::Rect::from_min_size(m_rect.max - egui::vec2(6.0, m_h), egui::vec2(6.0, m_h));
                            ui.painter().rect_filled(m_p_rect, 0.0, egui::Color32::from_rgb(0, 255, 180));
                        });
                    });
                }

                // Elegant Master Dashboard
                let m_rect = ui.allocate_exact_size(egui::vec2(main_w, 60.0), egui::Sense::hover()).0;
                ui.painter().rect_filled(m_rect, 0.0, egui::Color32::from_rgb(15, 15, 18));
                ui.child_ui(m_rect, egui::Layout::left_to_right(egui::Align::Center)).horizontal(|ui| {
                    ui.add_space(30.0);
                    ui.strong("MASTER");
                    if ui.add(egui::Slider::new(&mut self.master_gain, 0.0..=1.5).show_value(false)).changed() {
                        // PD-2: Corrected target_id for SummingProcessor (Master Gain)
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: 16,
                            param_id: 0,
                            value: self.master_gain,
                            ramp_duration_samples: 128,
                        });
                    }

                    let m_peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[16].min(1.2));
                    let (mtr_rect, _) = ui.allocate_exact_size(egui::vec2(250.0, 10.0), egui::Sense::hover());
                    ui.painter().rect_filled(mtr_rect, 1.0, egui::Color32::from_gray(25));
                    let m_w_val = (m_peak * 250.0).min(250.0);
                    ui.painter().rect_filled(egui::Rect::from_min_size(mtr_rect.min, egui::vec2(m_w_val, 10.0)), 1.0, egui::Color32::from_rgb(0, 180, 255));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(30.0);
                        ui.label(egui::RichText::new("128.0 BPM").strong().color(egui::Color32::from_gray(120)));
                        ui.separator();
                        let cpu_pct = telemetry.as_ref().map_or(0.0, |t| {
                            // Assume 3.0 GHz CPU for rough estimate
                            let budget_ns = (128.0 / 44100.0) * 1e9;
                            (t.process_time_ns as f64 / budget_ns * 100.0).min(100.0)
                        });
                        ui.label(egui::RichText::new(format!("CPU {:.0}%", cpu_pct)).small().color(egui::Color32::from_gray(80)));
                    });
                });
            });

            // --- TRACK LIBRARY ---
            ui.vertical(|ui| {
                ui.set_width(lib_w);
                egui::Frame::none().fill(egui::Color32::from_rgb(12, 12, 14)).inner_margin(12.0).show(ui, |ui| {
                    ui.label(egui::RichText::new("LIBRARY").color(egui::Color32::from_gray(150)).small().strong());
                    ui.add_space(10.0);
                    ui.text_edit_singleline(&mut self.search_query);
                    ui.add_space(15.0);

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let tracks = [
                            ("Deep Techno", "nullherz", 126.0),
                            ("Ambient Flow", "dsp_king", 90.0),
                            ("Glitch Hop", "rust_ace", 140.0),
                            ("Acid Bass", "tb_303", 128.0),
                            ("Minimal House", "logic_error", 124.0),
                            ("Rust Vibes", "ferris", 132.0),
                        ];
                        for (title, artist, bpm) in tracks {
                            // Filter by search query
                            if !self.search_query.is_empty() {
                                let q = self.search_query.to_lowercase();
                                if !title.to_lowercase().contains(&q) && !artist.to_lowercase().contains(&q) {
                                    continue;
                                }
                            }

                            let (rect, res) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::click());
                            let how_h = ui.ctx().animate_bool(res.id, res.hovered());
                            if how_h > 0.0 { ui.painter().rect_filled(rect, 0.0, egui::Color32::from_gray((how_h * 20.0) as u8)); }

                            ui.child_ui(rect, egui::Layout::left_to_right(egui::Align::Center)).horizontal(|ui| {
                                ui.add_space(5.0);
                                ui.label(egui::RichText::new(format!("{} - {}", title, artist)).size(11.0));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add_space(5.0);
                                    ui.label(egui::RichText::new(format!("{:.0}", bpm)).color(egui::Color32::from_gray(80)).size(10.0));
                                });
                            });
                        }
                    });
                });
            });
        });
    }

    fn render_topology(&self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        ui.heading("Engine Topology");
        ui.add_space(10.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
        let painter = ui.painter();
        let cell_w = rect.width() / 64.0;
        for i in 0..64 {
            let load = telemetry.as_ref().map_or(0.0, |t| (t.node_times_ns[i] as f32 / 500000.0).min(1.0));
            painter.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(i as f32 * cell_w, 0.0), egui::vec2(cell_w, 24.0)), 0.0, egui::Color32::from_rgb((load * 255.0) as u8, (255.0 * (1.0 - load)) as u8, 100));
        }
        ui.add_space(20.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, node) in self.graph.nodes.iter().enumerate() {
                ui.label(format!("Node {:02}: In {:?} Out {:?}", i, node.inputs, node.outputs));
            }
        });
    }

    fn render_mixer(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        ui.heading("Studio Console");
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            for i in 0..4 {
                ui.vertical_centered(|ui| {
                    ui.strong(format!("CH {}", i + 1));
                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*3 + 2].min(1.2));
                    if ui.add(egui::Slider::new(&mut self.channel_gains[i], 0.0..=1.2).vertical().show_value(false)).changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: (i as u64 * 3 + 2),
                            param_id: 0,
                            value: self.channel_gains[i],
                            ramp_duration_samples: 128,
                        });
                    }
                    ui.label(format!("{:.0}%", peak * 100.0));
                });
                ui.add_space(50.0);
            }
        });
    }

    fn render_sampler(&mut self, ui: &mut egui::Ui) {
        ui.heading("Production Sampler");
        ui.add_space(10.0);
        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.strong("SAMPLE BANK");
                for s in &self.sample_pool { ui.label(s); }
            });
            cols[1].group(|ui| {
                ui.strong("GRID SEQUENCER");
                ui.label("Beat Step Active");
            });
        });
    }

    fn render_mastering(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mastering Chain");
        ui.add_space(20.0);
        ui.group(|ui| {
            ui.checkbox(&mut self.mastering_eq_enabled, "LINEAR EQ");
            ui.checkbox(&mut self.mastering_comp_enabled, "DYNAMIC COMP");
            ui.checkbox(&mut self.mastering_limiter_enabled, "LIMITER");
        });
    }

    fn render_broadcast(&mut self, ui: &mut egui::Ui) {
        ui.heading("Live Broadcast Hub");
        ui.add_space(10.0);
        if ui.button(if self.is_streaming { "🛑 STOP STREAM" } else { "🚀 GO LIVE" }).clicked() { self.is_streaming = !self.is_streaming; }
        ui.label(format!("Status: {}", if self.is_streaming { "ONLINE" } else { "OFFLINE" }));
    }
}

impl eframe::App for InspectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let telemetry = *self.last_telemetry.lock().unwrap();

        egui::TopBottomPanel::top("nav").frame(egui::Frame::none().fill(egui::Color32::from_gray(5)).inner_margin(12.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(30.0, 0.0);
                for (view, label) in [
                    (View::Studio, "STUDIO"), (View::Mixer, "MIXER"), (View::Sampler, "SAMPLER"),
                    (View::Mastering, "MASTERING"), (View::Broadcast, "BROADCAST"), (View::Topology, "TOPOLOGY"),
                ] {
                    ui.selectable_value(&mut self.active_view, view, egui::RichText::new(label).small().strong());
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("nullherz").color(egui::Color32::from_rgb(0, 200, 255)).strong());
                    ui.add_space(15.0);
                    let color = egui::Color32::from_rgb(50, 255, 100);
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(60.0, 20.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, color.linear_multiply(0.2));
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "ON-AIR", egui::FontId::proportional(10.0), color);
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_view {
                View::Studio => self.render_dj_studio(ui, &telemetry),
                View::Mixer => self.render_mixer(ui, &telemetry),
                View::Sampler => self.render_sampler(ui),
                View::Topology => self.render_topology(ui, &telemetry),
                View::Mastering => self.render_mastering(ui),
                View::Broadcast => self.render_broadcast(ui),
            }
        });

        ctx.request_repaint();
    }
}
