mod widgets;
mod views;

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

    let (gui_mode, path) = if args.len() > 1 && args[1] == "--gui" {
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
    Performance,
    Mixer,
    Sampler,
    Modulation,
    Mastering,
    Broadcast,
    Settings,
    Topology,
    Arranger,
}

pub struct Track {
    pub title: String,
    pub artist: String,
    pub bpm: f32,
}

pub struct InspectorApp {
    graph: GraphJson,
    last_telemetry: Arc<Mutex<Option<Telemetry>>>,
    sample_registry: Arc<nullherz_dna::SampleRegistry>,
    command_sender: mpsc::Sender<nullherz_traits::Command>,
    active_view: View,

    // UI State — all controls bound to persistent state
    channel_faders: [f32; 4],
    channel_trims: [f32; 4],
    channel_eq_high: [f32; 4],
    channel_eq_mid: [f32; 4],
    channel_eq_low: [f32; 4],
    channel_fx_enabled: [bool; 4],
    channel_cue: [bool; 4],
    channel_sync: [bool; 4],
    quantize_enabled: bool,
    master_gain: f32,
    crossfader_pos: f32,
    library_db: nullherz_dna::LibraryDatabase,
    search_query: String,
    is_streaming: bool,
    library_visible: bool,
    selected_deck: usize,
    master_deck: Option<usize>,
    pitch_range: [f32; 4], // 0.08, 0.16, 1.0
    crossfader_curve: f32, // 0.0 = linear, 1.0 = power
    now_playing: [Option<String>; 4],
    global_bpm: f32,
    pitch_bend: [f32; 4],
    macros: [f32; 8],
    macro_names: [String; 8],

    // Peak hold
    channel_peak_hold: [f32; 4],
    master_peak_hold: f32,

    // Mastering chain state
    mastering_eq_enabled: bool,
    mastering_eq_low: f32,
    mastering_eq_mid: f32,
    mastering_eq_high: f32,
    mastering_comp_enabled: bool,
    mastering_comp_threshold: f32,
    mastering_comp_ratio: f32,
    mastering_comp_attack: f32,
    mastering_limiter_enabled: bool,
    mastering_limiter_gain: f32,
    mastering_limiter_lookahead: f32,

    // Sequencer state
    sequencer_grid: [[bool; 64]; 16],
    sequencer_active_step: usize,
}

impl InspectorApp {
    fn deck_color(i: usize) -> egui::Color32 {
        match i {
            0 => egui::Color32::from_rgb(0, 255, 200),   // Neon Mint (A)
            1 => egui::Color32::from_rgb(0, 180, 255),   // Electric Blue (B)
            2 => egui::Color32::from_rgb(255, 150, 0),   // Vibrant Orange (C)
            3 => egui::Color32::from_rgb(255, 50, 150),  // Hot Pink (D)
            _ => egui::Color32::WHITE,
        }
    }


    pub fn new(graph: GraphJson, cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.window_rounding = 0.0.into();

        let bg_deep = egui::Color32::from_rgb(8, 8, 10);
        let accent_primary = egui::Color32::from_rgb(0, 255, 200); // Neon Mint
        let stroke_dim = egui::Color32::from_gray(20);

        visuals.widgets.noninteractive.bg_fill = bg_deep;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, stroke_dim);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(15, 15, 18);
        visuals.widgets.inactive.rounding = 2.0.into();
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(25, 25, 30);
        visuals.widgets.active.bg_fill = accent_primary;

        visuals.selection.bg_fill = accent_primary.linear_multiply(0.2);
        cc.egui_ctx.set_visuals(visuals);

        // Adjust spacing for high-density "Pro" layout
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.visuals.window_rounding = 2.0.into();
        cc.egui_ctx.set_style(style);

        let last_telemetry = Arc::new(Mutex::new(None));
        let tel_clone = last_telemetry.clone();
        let sample_registry = Arc::new(nullherz_dna::SampleRegistry::new());
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
            sample_registry,
            command_sender: cmd_tx,
            active_view: View::Studio,
            channel_faders: [0.8; 4],
            channel_trims: [1.0; 4],
            channel_eq_high: [1.0; 4],
            channel_eq_mid: [1.0; 4],
            channel_eq_low: [1.0; 4],
            channel_fx_enabled: [false; 4],
            channel_cue: [false; 4],
            channel_sync: [false; 4],
            quantize_enabled: true,
            master_gain: 1.0,
            crossfader_pos: 0.5,
            library_db: nullherz_dna::LibraryDatabase::load("library.redb").expect("Failed to load library"),
            search_query: String::new(),
            is_streaming: false,
            library_visible: true,
            selected_deck: 0,
            master_deck: Some(0),
            pitch_range: [0.08; 4],
            crossfader_curve: 0.5,
            now_playing: [None, None, None, None],
            global_bpm: 128.0,
            pitch_bend: [1.0; 4],
            macros: [0.0; 8],
            macro_names: [
                "MACRO 1".to_string(), "MACRO 2".to_string(), "MACRO 3".to_string(), "MACRO 4".to_string(),
                "MACRO 5".to_string(), "MACRO 6".to_string(), "MACRO 7".to_string(), "MACRO 8".to_string(),
            ],
            channel_peak_hold: [0.0; 4],
            master_peak_hold: 0.0,
            mastering_eq_enabled: true,
            mastering_eq_low: 1.0,
            mastering_eq_mid: 1.0,
            mastering_eq_high: 1.0,
            mastering_comp_enabled: true,
            mastering_comp_threshold: 0.5,
            mastering_comp_ratio: 0.5,
            mastering_comp_attack: 0.2,
            mastering_limiter_enabled: false,
            mastering_limiter_gain: 1.0,
            mastering_limiter_lookahead: 0.5,
            sequencer_grid: [[false; 64]; 16],
            sequencer_active_step: 0,
        }
    }

    fn render_goniometer(&self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(100.0, 100.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(5, 5, 6));
        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(30)));

        // Draw crosshair
        ui.painter().vline(rect.center().x, rect.y_range(), egui::Stroke::new(0.5, egui::Color32::from_gray(20)));
        ui.painter().hline(rect.x_range(), rect.center().y, egui::Stroke::new(0.5, egui::Color32::from_gray(20)));

        if let Some(t) = telemetry {
            let time = ui.input(|i| i.time);
            let peak = t.peak_levels[21].min(1.0);

            let mut points = Vec::new();
            let num_points = 20;
            for i in 0..num_points {
                let phase = time * 5.0 + i as f64 * 0.1;
                let l = (phase.sin() * peak as f64) as f32;
                let r = ((phase * 1.1).cos() * peak as f64) as f32;

                // Rotate 45 degrees for Goniometer (L+R, L-R)
                let x = (l - r) * 0.707;
                let y = (l + r) * 0.707;

                points.push(rect.center() + egui::vec2(x * 40.0, -y * 40.0));
            }
            ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 255, 200))));
        }

        ui.painter().text(rect.min + egui::vec2(5.0, 5.0), egui::Align2::LEFT_TOP, "VECTORSCOPE", egui::FontId::proportional(7.0), egui::Color32::from_gray(100));
    }

    fn render_oscillator_monitor(&self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 160.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(5, 5, 6));
        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(20)));

        let time = ui.input(|i| i.time);
        let w = rect.width();
        let h = rect.height();

        // High-density background grid
        for i in 0..16 {
            let x = rect.min.x + (i as f32 * (w / 16.0));
            ui.painter().vline(x, rect.y_range(), egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 5)));
        }

        // Aggregate All-Deck Visualization
        for i in 0..4 {
            let color = Self::deck_color(i).linear_multiply(0.4);
            let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1]);
            let speed = 4.0 + (i as f32 * 2.0);
            let offset = i as f32 * 0.5;

            let points: Vec<egui::Pos2> = (0..w as i32).step_by(3).map(|x| {
                let px = x as f32 / w;
                let wave = (px * 20.0 + time as f32 * speed + offset).sin() * (px * 10.0 + time as f32).cos();
                let amp = (h * 0.3) * wave * peak.min(1.0);
                egui::pos2(rect.min.x + x as f32, rect.center().y + amp)
            }).collect();

            ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.5, color)));
        }

        ui.painter().text(rect.min + egui::vec2(10.0, 10.0), egui::Align2::LEFT_TOP, "WIDESCREEN OSCILLATOR MONITOR", egui::FontId::proportional(10.0), egui::Color32::from_gray(80));
    }


    fn render_performance_mode(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        let total_w = ui.available_width();

        // STAGE LIGHTING (REACTIVE BG)
        let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.0));
        let bg_color = egui::Color32::from_rgb(
            (peak * 15.0) as u8,
            (peak * 10.0) as u8,
            (peak * 20.0) as u8
        );

        ui.painter().rect_filled(ui.max_rect(), 0.0, bg_color);

        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(egui::RichText::new("LIVE STAGE").size(32.0).strong().color(egui::Color32::from_gray(200)));
            ui.add_space(40.0);

            ui.horizontal(|ui| {
                ui.add_space(total_w * 0.05);

                // DECK A
                ui.vertical(|ui| {
                    ui.set_width(total_w * 0.4);
                    self.render_deck(ui, 0, telemetry, 520.0);
                });

                ui.add_space(total_w * 0.05);

                // DECK B
                ui.vertical(|ui| {
                    ui.set_width(total_w * 0.4);
                    self.render_deck(ui, 1, telemetry, 520.0);
                });
            });

            ui.add_space(50.0);

            // GIANT TACTILE CROSSFADER
            let x_w = total_w * 0.7;
            ui.horizontal(|ui| {
                ui.add_space((total_w - x_w) / 2.0);
                ui.vertical(|ui| {
                    ui.set_width(x_w);
                    ui.label(egui::RichText::new("CROSSFADER").size(18.0).strong().color(egui::Color32::from_gray(100)));
                    let x_res = ui.add(egui::Slider::new(&mut self.crossfader_pos, 0.0..=1.0).show_value(false).handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.2 }));
                    if x_res.changed() {
                         for target_id in [16, 17] {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id, param_id: 0, value: self.crossfader_pos, ramp_duration_samples: 0 });
                        }
                    }
                });
            });

            // BEAT VISUALIZER
            ui.add_space(40.0);
            let time = ui.input(|i| i.time);
            let beat = (time * (self.global_bpm as f64 / 60.0)).fract();
            let (b_rect, _) = ui.allocate_exact_size(egui::vec2(x_w, 20.0), egui::Sense::hover());
            ui.painter().rect_filled(b_rect, 10.0, egui::Color32::from_gray(20));

            for i in 0..4 {
                 let bx = b_rect.min.x + (i as f32 * (x_w / 4.0)) + (x_w / 8.0);
                 let is_active = (beat * 4.0) as usize == i;
                 let b_color = if is_active { egui::Color32::from_rgb(0, 255, 200) } else { egui::Color32::from_gray(40) };
                 ui.painter().circle_filled(egui::pos2(bx, b_rect.center().y), 6.0, b_color);
            }
        });
    }

    fn render_dj_studio(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        let total_w = ui.available_width();
        let mixer_w = 320.0;
        let deck_w = (total_w - mixer_w - 48.0) / 4.0;

        ui.vertical(|ui| {
            ui.set_width(total_w);
            ui.add_space(5.0);

            // TOP: FULLSCREEN WIDE OSCILLATOR MONITOR
            self.render_oscillator_monitor(ui, telemetry);
            ui.add_space(10.0);

            // MACRO ROW
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(egui::RichText::new("MACROS").small().strong().color(egui::Color32::from_gray(120)));
                ui.add_space(20.0);
                for i in 0..8 {
                    ui.vertical(|ui| {
                        ui.set_width(60.0);
                        if widgets::render_knob(ui, &mut self.macros[i], 0.0..=1.0, "", egui::Color32::from_rgb(0, 255, 200)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetMacro {
                                macro_id: i as u32,
                                value: self.macros[i],
                            });
                        }
                        ui.label(egui::RichText::new(&self.macro_names[i]).size(7.0).color(egui::Color32::from_gray(100)));
                    });
                    ui.add_space(10.0);
                }
            });
            ui.add_space(10.0);

            // PERFORMANCE ROW: A, B, MIXER, C, D
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);

                // DECK A
                ui.vertical(|ui| { ui.set_width(deck_w); self.render_deck(ui, 0, telemetry, 420.0); });
                // DECK B
                ui.vertical(|ui| { ui.set_width(deck_w); self.render_deck(ui, 1, telemetry, 420.0); });

                // ULTRA-COMPACT CENTRAL MIXER
                ui.vertical(|ui| {
                    ui.set_width(mixer_w);
                    self.render_central_mixer(ui, telemetry, mixer_w);
                });

                // DECK C
                ui.vertical(|ui| { ui.set_width(deck_w); self.render_deck(ui, 2, telemetry, 420.0); });
                // DECK D
                ui.vertical(|ui| { ui.set_width(deck_w); self.render_deck(ui, 3, telemetry, 420.0); });
            });

            ui.add_space(20.0);

            // INTEGRATED MASTER & STATUS DASHBOARD
            ui.horizontal(|ui| {
                let dash_w = 400.0;
                let (d_rect, _) = ui.allocate_exact_size(egui::vec2(dash_w, 80.0), egui::Sense::hover());

                // Dashboard Background (Vented Rack Look)
                ui.painter().rect_filled(d_rect, 2.0, egui::Color32::from_rgb(15, 15, 18));
                ui.painter().rect_stroke(d_rect, 2.0, egui::Stroke::new(1.0, egui::Color32::from_gray(40)));

                // Subtle vent lines
                for i in 0..5 {
                    let y = d_rect.min.y + 5.0 + i as f32 * 15.0;
                    ui.painter().hline(d_rect.max.x - 60.0..=d_rect.max.x - 10.0, y, egui::Stroke::new(1.0, egui::Color32::from_gray(25)));
                }

                ui.child_ui(d_rect, egui::Layout::left_to_right(egui::Align::Center)).horizontal(|ui| {
                    ui.add_space(15.0);

                    // ENGINE TELEMETRY
                    ui.vertical(|ui| {
                        ui.add_space(15.0);
                        let cpu_pct = telemetry.as_ref().map_or(0.0, |t| {
                            let budget_ns = (128.0 / 44100.0) * 1e9;
                            (t.process_time_ns as f64 / budget_ns * 100.0).min(100.0)
                        });
                        ui.label(egui::RichText::new("ENGINE LOAD").color(egui::Color32::from_gray(100)).size(9.0).strong());
                        ui.label(egui::RichText::new(format!("{:.1}%", cpu_pct)).monospace().size(18.0).color(if cpu_pct > 80.0 { egui::Color32::RED } else { egui::Color32::from_rgb(0, 255, 200) }));
                    });

                    ui.add_space(30.0);

                    // GLOBAL CLOCK
                    ui.vertical(|ui| {
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new("GLOBAL BPM").color(egui::Color32::from_gray(100)).size(9.0).strong());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(2.0, 2.0);
                            if ui.button("-").clicked() { self.global_bpm -= 1.0; }
                            ui.add(egui::DragValue::new(&mut self.global_bpm).speed(0.1).fixed_decimals(1).custom_formatter(|n, _| format!("{:.1}", n)));
                            if ui.button("+").clicked() { self.global_bpm += 1.0; }
                        });

                        let q_color = if self.quantize_enabled { egui::Color32::from_rgb(255, 50, 50) } else { egui::Color32::from_gray(40) };
                        if ui.add(egui::Button::new(egui::RichText::new("QUANTIZE").small().strong().color(q_color)).frame(false)).clicked() {
                            self.quantize_enabled = !self.quantize_enabled;
                        }
                    });

                    ui.add_space(30.0);

                    // X-RUNS / STABILITY
                    ui.vertical(|ui| {
                        ui.add_space(15.0);
                        let xruns = telemetry.as_ref().map_or(0, |t| t.xrun_count);
                        ui.label(egui::RichText::new("X-RUNS").color(egui::Color32::from_gray(100)).size(9.0).strong());
                        ui.label(egui::RichText::new(format!("{:03}", xruns)).monospace().size(18.0).color(if xruns > 0 { egui::Color32::from_rgb(255, 150, 0) } else { egui::Color32::from_gray(50) }));
                    });
                });

                ui.add_space(ui.available_width() - 535.0); // Calibrated offset for master panel
                self.render_master_panel(ui, telemetry);
            });
        });
    }


    fn render_deck(&mut self, ui: &mut egui::Ui, i: usize, telemetry: &Option<Telemetry>, height: f32) {
        let deck_name = format!("DECK {}", (b'A' + i as u8) as char);
        let color = Self::deck_color(i);

        let is_selected = self.selected_deck == i;
        let stroke_color = if is_selected { color } else { egui::Color32::from_gray(30) };

        let rect = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::click()).0;
        if ui.rect_contains_pointer(rect) && ui.input(|i| i.pointer.any_click()) {
            self.selected_deck = i;
        }

        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(if is_selected { 2.0 } else { 1.0 }, stroke_color));
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(10, 10, 12));

        ui.child_ui(rect, egui::Layout::top_down(egui::Align::Center)).vertical(|ui| {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(deck_name).color(if is_selected { color } else { egui::Color32::from_gray(180) }).strong());

                let is_master = self.master_deck == Some(i);
                let master_color = if is_master { color } else { egui::Color32::from_gray(40) };
                if ui.add(egui::Button::new(egui::RichText::new("MASTER").small().strong().color(master_color)).frame(true)).clicked() {
                    if is_master { self.master_deck = None; } else { self.master_deck = Some(i); }
                }

                if let Some(sample) = self.sample_registry.get(i as u64 * 4) {
                    let key_text = if let Some(key) = sample.metadata.root_key {
                        let notes = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
                        format!("Key: {}", notes[(key.round() as usize) % 12])
                    } else {
                        "Key: -".to_string()
                    };
                    ui.label(egui::RichText::new(format!("{} | {:.1} BPM", key_text, sample.metadata.bpm)).small().color(egui::Color32::from_gray(100)));
                    ui.label(egui::RichText::new(format!("{:.1} BPM", sample.metadata.bpm)).small().color(color));
                    if let Some(key) = sample.metadata.root_key {
                        ui.label(egui::RichText::new(format!("Key: {:.0}", key)).small().color(egui::Color32::from_gray(100)));
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1]);
                    let db = 20.0 * peak.log10().max(-60.0);
                    ui.label(egui::RichText::new(format!("{:.1} dB", db)).small().color(if peak > 1.0 { egui::Color32::RED } else { egui::Color32::from_gray(100) }));
                });
            });

            ui.horizontal(|ui| {
                ui.add_space(10.0);
                egui::Frame::none().fill(egui::Color32::from_rgb(5, 5, 6)).inner_margin(4.0).rounding(2.0).show(ui, |ui| {
                    ui.set_width(ui.available_width() - 80.0);
                    if let Some(ref title) = self.now_playing[i] {
                        ui.label(egui::RichText::new(title).color(color).size(11.0).strong());
                    } else {
                        ui.label(egui::RichText::new("EMPTY").color(egui::Color32::from_gray(40)).size(11.0));
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    // REMAINING TIME SIMULATION
                    let total_sec = 324.0;
                    let elapsed = (ui.input(|i| i.time) % total_sec as f64) as f32;
                    let remaining = total_sec - elapsed;
                    let mins = (remaining / 60.0) as i32;
                    let secs = (remaining % 60.0) as i32;
                    ui.label(egui::RichText::new(format!("-{:02}:{:02}", mins, secs)).color(color).monospace().size(12.0));
                });
            });

            // WAVEFORM
            ui.add_space(10.0);
            let (w_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width() - 20.0, 120.0), egui::Sense::hover());
            ui.painter().rect_filled(w_rect, 2.0, egui::Color32::from_rgb(5, 5, 6));

            let w_width = w_rect.width();
            let w_height = w_rect.height();

            let time = ui.input(|i| i.time);
            if let Some(sample) = self.sample_registry.get(i as u64 * 4) {
                let peaks = &sample.metadata.peaks;
                if !peaks.is_empty() {
                    // Spectral Waveform Simulation: Layered Frequencies
                    for (layer, scale, l_color) in [
                        (0, 1.0, color),
                        (1, 0.6, color.linear_multiply(0.5)),
                        (2, 0.3, egui::Color32::WHITE.linear_multiply(0.2)),
                    ] {
                        let mut points = Vec::new();
                        for x in 0..w_width as usize {
                            let p_idx = (x * peaks.len()) / w_width as usize;
                            let val = peaks[p_idx.min(peaks.len() - 1)] * scale;
                            let y_off = val * (w_height / 2.0);
                            points.push(egui::pos2(w_rect.min.x + x as f32, w_rect.center().y - y_off));
                            points.push(egui::pos2(w_rect.min.x + x as f32, w_rect.center().y + y_off));
                        }
                        ui.painter().add(egui::Shape::line(points, egui::Stroke::new(if layer == 0 { 1.5 } else { 1.0 }, l_color)));
                    }

                    // Hot Cues
                    for (idx, cue) in sample.metadata.hot_cues.iter().enumerate() {
                        if let Some(pos) = cue {
                             let x_off = (*pos as f32 / sample.buffer.len() as f32) * w_width;
                             let tx = w_rect.min.x + x_off;
                             ui.painter().vline(tx, w_rect.y_range(), egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 255, 0)));
                             ui.painter().text(egui::pos2(tx, w_rect.min.y + 5.0), egui::Align2::LEFT_TOP, format!("{}", idx+1), egui::FontId::proportional(8.0), egui::Color32::WHITE);
                        }
                    }

                    // DETAIL SCROLLING WAVEFORM
                    ui.add_space(5.0);
                    let (d_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width() - 20.0, 40.0), egui::Sense::hover());
                    ui.painter().rect_filled(d_rect, 2.0, egui::Color32::from_rgb(5, 5, 6));

                    let dw = d_rect.width();
                    let dh = d_rect.height();
                    let play_head_norm = (time as f32 % 30.0) / 30.0; // Simulated playhead
                    let view_range = 0.05; // 5% of track

                    let mut d_points = Vec::new();
                    for x in 0..dw as usize {
                        let rel_x = x as f32 / dw;
                        let track_norm = (play_head_norm - view_range/2.0 + rel_x * view_range).clamp(0.0, 1.0);
                        let p_idx = (track_norm * peaks.len() as f32) as usize;
                        let val = peaks[p_idx.min(peaks.len() - 1)];
                        let y_off = val * (dh / 2.0);
                        d_points.push(egui::pos2(d_rect.min.x + x as f32, d_rect.center().y - y_off));
                        d_points.push(egui::pos2(d_rect.min.x + x as f32, d_rect.center().y + y_off));
                    }
                    ui.painter().add(egui::Shape::line(d_points, egui::Stroke::new(1.5, color.additive())));
                    ui.painter().vline(d_rect.center().x, d_rect.y_range(), egui::Stroke::new(2.0, egui::Color32::WHITE));
                }
            } else {
                // FALLBACK Simulation if no real data
                let time = ui.input(|i| i.time);
                for (band, speed, scale) in [ (0, 8.0, 15.0), (1, 12.0, 10.0), (2, 20.0, 5.0) ] {
                    let points: Vec<egui::Pos2> = (0..w_width as i32).step_by(2).map(|x| {
                        let phase = x as f32 * 0.05 * (band as f32 + 1.0) + (time * speed) as f32;
                        let amp = scale * ((phase * 0.01).cos().abs() + 0.2);
                        egui::pos2(w_rect.min.x + x as f32, w_rect.center().y + (phase.sin() * amp))
                    }).collect();
                    ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.2, color.linear_multiply(0.8 - band as f32 * 0.2))));
                }
            }

            // High-Visibility Beat Markers
            for b in 0..8 {
                let bx = w_rect.min.x + (w_width * (b as f32 / 8.0));
                ui.painter().vline(bx, w_rect.y_range(), egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 30)));
            }

            // Transient Metadata Overlay
            if let Some(sample) = self.sample_registry.get(i as u64 * 4) {
                let total_samples = sample.buffer.len() as f32;
                if total_samples > 0.0 {
                    for &pos in sample.metadata.transients.iter() {
                        let x_off = (pos as f32 / total_samples).fract(); // Simple wrap for simulation
                        let tx = w_rect.min.x + (x_off * w_width);
                        ui.painter().vline(tx, w_rect.y_range(), egui::Stroke::new(1.5, color.linear_multiply(0.5)));
                    }
                }
            }

            // Central Playhead
            ui.painter().vline(w_rect.center().x, w_rect.y_range(), egui::Stroke::new(2.5, egui::Color32::from_rgb(255, 255, 255)));

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.add_space(15.0);
                // JOG SIMULATION (Simplified)
                let (jog_rect, _) = ui.allocate_exact_size(egui::vec2(100.0, 100.0), egui::Sense::hover());
                ui.painter().circle_stroke(jog_rect.center(), 50.0, egui::Stroke::new(2.0, egui::Color32::from_gray(40)));
                let angle = (time * 2.0) as f32;
                let needle = jog_rect.center() + egui::vec2(angle.cos() * 45.0, angle.sin() * 45.0);
                ui.painter().line_segment([jog_rect.center(), needle], egui::Stroke::new(2.0, color));

                ui.add_space(15.0);

                ui.vertical(|ui| {
                    let cue_color = if self.channel_cue[i] { egui::Color32::from_rgb(255, 150, 0) } else { egui::Color32::from_gray(40) };
                    if ui.add(egui::Button::new(egui::RichText::new("CUE").color(cue_color).strong()).min_size(egui::vec2(60.0, 45.0))).clicked() {
                        self.channel_cue[i] = !self.channel_cue[i];
                    }
                    ui.add_space(8.0);
                    let play_color = egui::Color32::from_rgb(0, 255, 100);
                    if ui.add(egui::Button::new(egui::RichText::new("PLAY").color(play_color).strong()).min_size(egui::vec2(60.0, 45.0))).clicked() {
                        // Play logic
                    }
                });

                ui.add_space(15.0);

                ui.vertical(|ui| {
                    let s_color = if self.channel_sync[i] { color } else { egui::Color32::from_gray(40) };
                    if ui.add(egui::Button::new(egui::RichText::new("SYNC").color(s_color).strong()).min_size(egui::vec2(60.0, 45.0))).clicked() {
                        self.channel_sync[i] = !self.channel_sync[i];
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: (i as u64 * 4),
                            param_id: 2, // Sync/Quantize toggle
                            value: if self.channel_sync[i] { 1.0 } else { 0.0 },
                            ramp_duration_samples: 0,
                        });
                    }
                });

                ui.add_space(15.0);

                // PITCH FADER
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("PITCH").size(9.0).strong().color(egui::Color32::from_gray(100)));

                    egui::ComboBox::from_id_source(format!("pitch_range_{}", i))
                        .selected_text(format!("{:.0}%", self.pitch_range[i] * 100.0))
                        .width(40.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.pitch_range[i], 0.08, "8%");
                            ui.selectable_value(&mut self.pitch_range[i], 0.16, "16%");
                            ui.selectable_value(&mut self.pitch_range[i], 1.0, "WIDE");
                        });

                    let range = (1.0 - self.pitch_range[i])..=(1.0 + self.pitch_range[i]);
                    let p_res = ui.add(egui::Slider::new(&mut self.pitch_bend[i], range).vertical().show_value(false).handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 4.0 }));
                    if p_res.changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: (i as u64 * 4), // Targeting Sampler node
                            param_id: 1, // Assume 1 is playback rate
                            value: self.pitch_bend[i],
                            ramp_duration_samples: 128,
                        });
                    }
                    let pct = (self.pitch_bend[i] - 1.0) * 100.0;
                    ui.label(egui::RichText::new(format!("{:+.1}%", pct)).size(10.0).monospace().color(color));
                });
            });

            // PHASE METER
            ui.add_space(5.0);
            let (p_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width() - 30.0, 12.0), egui::Sense::hover());
            ui.painter().rect_filled(p_rect, 1.0, egui::Color32::from_rgb(5, 5, 6));
            ui.painter().vline(p_rect.center().x, p_rect.y_range(), egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

            if self.channel_sync[i] {
                let phase_diff = (time.sin() * 0.5) as f32; // Simulated phase diff
                let px = p_rect.center().x + (phase_diff * p_rect.width() * 0.4);
                ui.painter().circle_filled(egui::pos2(px, p_rect.center().y), 4.0, color);
            }
        });
    }

    fn render_spectrum_analyzer(&self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(200.0, 100.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(10, 10, 12));
        ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, egui::Color32::from_gray(30)));

        let time = ui.input(|i| i.time);
        let num_bins = 32;
        let bin_w = rect.width() / num_bins as f32;

        for i in 0..num_bins {
            let h_val = if let Some(t) = telemetry {
                // Simulate spectrum from peak levels + some noise/oscillation
                let base = t.peak_levels[21].min(1.0);
                let noise = ((time * 10.0 + i as f64 * 0.5).sin() * 0.2 + 0.8) as f32;
                let freq_falloff = 1.0 - (i as f32 / num_bins as f32).powi(2);
                base * noise * freq_falloff
            } else {
                0.0
            };

            let h = h_val * rect.height();
            let bin_rect = egui::Rect::from_min_size(
                egui::pos2(rect.min.x + i as f32 * bin_w, rect.max.y - h),
                egui::vec2(bin_w - 1.0, h)
            );

            let color = egui::Color32::from_rgb(0, (255.0 * (1.0 - i as f32 / num_bins as f32)) as u8, 200);
            ui.painter().rect_filled(bin_rect, 0.0, color.linear_multiply(0.8));
        }
    }

    fn render_master_panel(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        egui::Frame::none().fill(egui::Color32::from_rgb(25, 25, 30)).rounding(4.0).stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(40))).inner_margin(12.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                self.render_goniometer(ui, telemetry);
                ui.add_space(10.0);
                self.render_spectrum_analyzer(ui, telemetry);
                ui.add_space(15.0);

                ui.vertical(|ui| {
                    ui.set_width(70.0);
                    if widgets::render_knob(ui, &mut self.mastering_limiter_gain, 0.0..=2.0, "CEIL", egui::Color32::from_rgb(0, 255, 200)).changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 21, param_id: 0, value: self.mastering_limiter_gain, ramp_duration_samples: 128 });
                    }
                    ui.add_space(10.0);
                    if widgets::render_knob(ui, &mut self.mastering_comp_threshold, 0.0..=1.0, "THR", egui::Color32::from_rgb(0, 255, 200)).changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 20, param_id: 0, value: self.mastering_comp_threshold, ramp_duration_samples: 128 });
                    }
                });

                ui.add_space(15.0);

                let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2));
                if peak > self.master_peak_hold { self.master_peak_hold = peak; }
                else { self.master_peak_hold *= 0.98; }

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                    for _ in 0..2 {
                        widgets::render_vu_meter(ui, peak, self.master_peak_hold, egui::Color32::from_rgb(0, 255, 180), 100.0);
                    }
                });

                ui.add_space(10.0);

                if widgets::render_fader(ui, &mut self.master_gain, 0.0..=1.5, egui::Color32::from_rgb(0, 255, 180)).changed() {
                    let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 21, param_id: 0, value: self.master_gain, ramp_duration_samples: 128 });
                }
            });
        });
    }

    fn render_central_mixer(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>, main_w: f32) {
        let rect = ui.allocate_exact_size(egui::vec2(main_w, 420.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(15, 15, 18));
        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(30)));

        ui.child_ui(rect, egui::Layout::top_down(egui::Align::Center)).vertical_centered(|ui| {
            ui.add_space(10.0);

            // CHANNEL STRIPS
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(12.0, 0.0);
                for i in 0..4 {
                    ui.vertical(|ui| {
                        ui.set_width(60.0);

                        // GAIN / TRIM
                        if widgets::render_knob(ui, &mut self.channel_trims[i], 0.0..=2.0, "TRIM", Self::deck_color(i)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                target_id: (i as u64 * 4 + 1),
                                param_id: 0,
                                value: self.channel_trims[i] * self.channel_faders[i],
                                ramp_duration_samples: 128,
                            });
                        }

                        ui.add_space(10.0);

                        // HI / MID / LOW
                        for (label, param_idx, state_val) in [("HI", 2, &mut self.channel_eq_high[i]), ("MID", 1, &mut self.channel_eq_mid[i]), ("LOW", 0, &mut self.channel_eq_low[i])] {
                            if widgets::render_knob(ui, state_val, 0.0..=2.0, label, Self::deck_color(i)).changed() {
                                let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                    target_id: (i as u64 * 4 + 3),
                                    param_id: param_idx,
                                    value: *state_val,
                                    ramp_duration_samples: 0,
                                });
                            }
                            ui.add_space(6.0);
                        }

                        ui.add_space(8.0);

                        // FX BUTTON
                        let fx_color = if self.channel_fx_enabled[i] { egui::Color32::from_rgb(0, 255, 200) } else { egui::Color32::from_gray(40) };
                        if ui.add(egui::Button::new(egui::RichText::new("FX").color(fx_color).small().strong()).min_size(egui::vec2(40.0, 24.0))).clicked() {
                            self.channel_fx_enabled[i] = !self.channel_fx_enabled[i];
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                target_id: (i as u64 * 4 + 2),
                                param_id: 999,
                                value: if self.channel_fx_enabled[i] { 1.0 } else { 0.0 },
                                ramp_duration_samples: 0,
                            });
                        }

                        ui.add_space(12.0);

                        // FADER & VU
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
                            if widgets::render_fader(ui, &mut self.channel_faders[i], 0.0..=1.0, Self::deck_color(i)).changed() {
                                let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                    target_id: (i as u64 * 4 + 1),
                                    param_id: 0,
                                    value: self.channel_trims[i] * self.channel_faders[i],
                                    ramp_duration_samples: 128,
                                });
                            }

                            // VU METER
                            let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1].min(1.2));
                            if peak > self.channel_peak_hold[i] { self.channel_peak_hold[i] = peak; }
                            else { self.channel_peak_hold[i] *= 0.98; }

                            widgets::render_vu_meter(ui, peak, self.channel_peak_hold[i], egui::Color32::from_rgb(0, 255, 180), 120.0);
                        });
                    });
                }
            });

            ui.add_space(20.0);

            ui.add_space(15.0);

            // GLOBAL CROSSFADER (Centered below faders)
            ui.vertical_centered(|ui| {
                ui.set_width(main_w);
                ui.horizontal(|ui| {
                    ui.add_space(main_w / 2.0 - 40.0);
                    ui.label(egui::RichText::new("X-FADE").small().strong().color(egui::Color32::from_gray(100)));
                    if ui.add(egui::Button::new(if self.crossfader_curve > 0.5 { "POWER" } else { "LIN" }).small()).clicked() {
                        self.crossfader_curve = if self.crossfader_curve > 0.5 { 0.0 } else { 1.0 };
                        for target_id in [16, 17] {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id, param_id: 1, value: self.crossfader_curve, ramp_duration_samples: 0 });
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("A").color(egui::Color32::from_rgb(0, 200, 255)));
                    ui.spacing_mut().slider_width = main_w - 60.0;
                    let x_slider = ui.add(egui::Slider::new(&mut self.crossfader_pos, 0.0..=1.0).show_value(false).handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.5 }));
                    if x_slider.changed() {
                        for target_id in [16, 17] {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id, param_id: 0, value: self.crossfader_pos, ramp_duration_samples: 0 });
                        }
                    }
                    ui.label(egui::RichText::new("B").color(egui::Color32::from_rgb(0, 255, 150)));
                });
            });
        });
    }

    fn render_sampler(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        ui.horizontal(|ui| {
            ui.heading("Production Sampler");
            ui.add_space(20.0);
            ui.label(egui::RichText::new("GRID SEQUENCER (16x64)").color(egui::Color32::from_gray(100)));
        });
        ui.add_space(10.0);

        if let Some(t) = telemetry {
            // Assume 44100 Hz, 120 BPM (0.5s per beat), 1/16th note = 0.125s = 5512.5 samples
            // For a more robust solution, we'd use global_bpm, but let's approximate:
            let samples_per_step = (44100.0 * 60.0 / self.global_bpm / 4.0) as u64;
            self.sequencer_active_step = (t.sample_counter / samples_per_step.max(1)) as usize % 64;
        } else {
             let time = ui.input(|i| i.time);
             self.sequencer_active_step = (time * 8.0) as usize % 64;
        }

        egui::Frame::none().fill(egui::Color32::from_rgb(10, 10, 12)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                ui.vertical(|ui| {
                    for row in 0..16 {
                        ui.horizontal(|ui| {
                            ui.set_height(20.0);
                            let track_color = if row % 4 == 0 { egui::Color32::from_rgb(0, 255, 200) } else { egui::Color32::from_gray(60) };
                            ui.label(egui::RichText::new(format!("TRK {:02}", row+1)).color(track_color).size(10.0).monospace());
                            ui.add_space(10.0);

                            for step in 0..64 {
                                let (rect, res) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
                                if res.clicked() {
                                    self.sequencer_grid[row][step] = !self.sequencer_grid[row][step];
                                    let _ = self.command_sender.send(nullherz_traits::Command::SetSequencerStep {
                                        node_idx: 100, // Target default sequencer node
                                        track: row as u32,
                                        step: step as u32,
                                        value: self.sequencer_grid[row][step],
                                    });
                                }

                                let is_active = self.sequencer_grid[row][step];
                                let is_current = self.sequencer_active_step == step;

                                let mut bg_color = if is_active { track_color } else { egui::Color32::from_gray(25) };
                                if is_current { bg_color = bg_color.additive(); }

                                let stroke = if is_current {
                                    egui::Stroke::new(2.0, egui::Color32::WHITE)
                                } else if step % 4 == 0 {
                                    egui::Stroke::new(1.0, egui::Color32::from_gray(40))
                                } else {
                                    egui::Stroke::new(0.5, egui::Color32::from_gray(30))
                                };

                                ui.painter().rect_filled(rect, 1.0, bg_color);
                                ui.painter().rect_stroke(rect, 1.0, stroke);

                                if is_current {
                                    ui.painter().rect_filled(rect.expand(1.0), 1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 40));
                                }
                            }
                        });
                        ui.add_space(2.0);
                    }
                });
            });
        });

        ui.add_space(20.0);
        ui.group(|ui| {
            ui.strong("SEQUENCER SETTINGS");
            ui.horizontal(|ui| {
                ui.label("Steps: 64");
                ui.add_space(20.0);
                ui.label("Resolution: 1/16");
                ui.add_space(20.0);
                if ui.button("CLEAR ALL").clicked() {
                    self.sequencer_grid = [[false; 64]; 16];
                }
            });
        });
    }

    fn render_mastering(&mut self, ui: &mut egui::Ui, _telemetry: &Option<Telemetry>) {
        ui.heading("Precision Mastering Console");
        ui.add_space(20.0);

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(20.0, 0.0);

            // EQ MODULE
            egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(15.0).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("LINEAR PHASE EQ").color(egui::Color32::from_rgb(0, 255, 200)).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut self.mastering_eq_enabled, "");
                        });
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if widgets::render_knob(ui, &mut self.mastering_eq_low, 0.0..=2.0, "LOW", egui::Color32::from_rgb(0, 255, 200)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 19, param_id: 0, value: self.mastering_eq_low, ramp_duration_samples: 128 });
                        }
                        if widgets::render_knob(ui, &mut self.mastering_eq_mid, 0.0..=2.0, "MID", egui::Color32::from_rgb(0, 255, 200)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 19, param_id: 1, value: self.mastering_eq_mid, ramp_duration_samples: 128 });
                        }
                        if widgets::render_knob(ui, &mut self.mastering_eq_high, 0.0..=2.0, "HIGH", egui::Color32::from_rgb(0, 255, 200)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 19, param_id: 2, value: self.mastering_eq_high, ramp_duration_samples: 128 });
                        }
                    });
                });
            });

            // COMPRESSOR MODULE
            egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(15.0).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("BUS COMPRESSOR").color(egui::Color32::from_rgb(255, 180, 0)).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut self.mastering_comp_enabled, "");
                        });
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if widgets::render_knob(ui, &mut self.mastering_comp_threshold, 0.0..=1.0, "THR", egui::Color32::from_rgb(255, 180, 0)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 20, param_id: 0, value: self.mastering_comp_threshold, ramp_duration_samples: 128 });
                        }
                        if widgets::render_knob(ui, &mut self.mastering_comp_ratio, 0.0..=1.0, "RATIO", egui::Color32::from_rgb(255, 180, 0)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 20, param_id: 1, value: self.mastering_comp_ratio, ramp_duration_samples: 128 });
                        }
                        if widgets::render_knob(ui, &mut self.mastering_comp_attack, 0.0..=1.0, "ATTACK", egui::Color32::from_rgb(255, 180, 0)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 20, param_id: 2, value: self.mastering_comp_attack, ramp_duration_samples: 128 });
                        }

                        ui.add_space(10.0);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("GR").size(8.0).color(egui::Color32::from_gray(100)));
                            let gr = if self.mastering_comp_enabled { (self.mastering_comp_threshold * 0.5).sin().abs() } else { 0.0 };
                            widgets::render_vu_meter(ui, gr, gr, egui::Color32::RED, 60.0);
                        });
                    });
                });
            });

            // LIMITER MODULE
            egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(15.0).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("BRICKWALL LIMITER").color(egui::Color32::from_rgb(255, 50, 50)).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut self.mastering_limiter_enabled, "");
                        });
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if widgets::render_knob(ui, &mut self.mastering_limiter_gain, 0.0..=1.5, "CEIL", egui::Color32::from_rgb(255, 50, 50)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 21, param_id: 0, value: self.mastering_limiter_gain, ramp_duration_samples: 128 });
                        }
                        if widgets::render_knob(ui, &mut self.mastering_limiter_lookahead, 0.0..=1.0, "LOOK", egui::Color32::from_rgb(255, 50, 50)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 21, param_id: 1, value: self.mastering_limiter_lookahead, ramp_duration_samples: 128 });
                        }
                    });
                });
            });
        });

        ui.add_space(30.0);
        egui::Frame::none().fill(egui::Color32::from_rgb(10, 10, 12)).inner_margin(20.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("MASTER SIGNAL FLOW").color(egui::Color32::from_gray(80)).size(10.0));
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                for (name, color) in [("IN", egui::Color32::WHITE), ("EQ", egui::Color32::from_rgb(0, 255, 200)), ("COMP", egui::Color32::from_rgb(255, 180, 0)), ("LIMIT", egui::Color32::from_rgb(255, 50, 50)), ("OUT", egui::Color32::WHITE)] {
                    ui.label(egui::RichText::new(name).color(color).strong());
                    if name != "OUT" {
                        ui.label(egui::RichText::new(" ➔ ").color(egui::Color32::from_gray(40)));
                    }
                }
            });
        });
    }

    fn render_broadcast(&mut self, ui: &mut egui::Ui) {
        ui.heading("Live Broadcast Hub");
        ui.add_space(10.0);
        if ui.button(if self.is_streaming { "🛑 STOP STREAM" } else { "🚀 GO LIVE" }).clicked() { self.is_streaming = !self.is_streaming; }
        ui.label(format!("Status: {}", if self.is_streaming { "ONLINE" } else { "OFFLINE" }));
    }

    fn render_arranger(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        ui.heading("Song Arranger");
        ui.add_space(10.0);

        let current_beat = telemetry.as_ref().map_or(0.0, |t| t.sample_counter as f64 / (44100.0 * 60.0 / self.global_bpm as f64));
        ui.label(format!("Current Beat: {:.2}", current_beat));

        ui.add_space(20.0);
        ui.label("Timeline (Mockup)");
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 200.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(15, 15, 18));

        // Draw grid
        for i in 0..32 {
            let x = rect.min.x + (i as f32 * (rect.width() / 32.0));
            let color = if i % 4 == 0 { egui::Color32::from_gray(60) } else { egui::Color32::from_gray(30) };
            ui.painter().vline(x, rect.y_range(), egui::Stroke::new(1.0, color));
        }

        // Playhead
        let ph_x = rect.min.x + (current_beat.fract() as f32 * rect.width());
        ui.painter().vline(ph_x, rect.y_range(), egui::Stroke::new(2.0, egui::Color32::RED));

        ui.add_space(20.0);
        ui.group(|ui| {
            ui.label("Add Arrangement Event");
            ui.horizontal(|ui| {
                ui.button("Trigger Pattern A on Deck 1 at Beat 16");
                ui.button("Set Macro 1 to 0.8 at Beat 32");
            });
        });
    }
}

impl eframe::App for InspectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let telemetry = *self.last_telemetry.lock().unwrap();

        // LEFT-ALIGNED VERTICAL NAVIGATION
        egui::SidePanel::left("nav_panel")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(8, 8, 10)).inner_margin(12.0))
            .width_range(60.0..=60.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("NH").color(egui::Color32::from_rgb(0, 255, 200)).strong().size(20.0));
                    ui.add_space(30.0);

                    for (view, label, icon) in [
                        (View::Studio, "STUDIO", "🎧"),
                        (View::Performance, "STAGE", "✨"),
                        (View::Mixer, "MIXER", "🎚"),
                        (View::Sampler, "SAMPLER", "🎹"),
                        (View::Modulation, "MOD MATRIX", "🔄"),
                        (View::Mastering, "MASTERING", "💎"),
                        (View::Broadcast, "BROADCAST", "📡"),
                        (View::Settings, "SETTINGS", "⚙"),
                        (View::Arranger, "ARRANGER", "📊"),
                        (View::Topology, "TOPOLOGY", "🕸"),
                    ] {
                        let is_active = self.active_view == view;
                        let color = if is_active { egui::Color32::from_rgb(0, 255, 200) } else { egui::Color32::from_gray(100) };

                        let (rect, res) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::click());
                        if res.clicked() { self.active_view = view; }

                        if is_active {
                             ui.painter().rect_filled(rect.expand(2.0), 4.0, color.linear_multiply(0.05));
                             ui.painter().vline(rect.min.x - 8.0, rect.y_range(), egui::Stroke::new(2.0, color));
                        }

                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(20.0), color);
                        res.on_hover_text(label);

                        ui.add_space(15.0);
                    }

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        if ui.add(egui::Button::new(egui::RichText::new(if self.library_visible { "📁" } else { "📂" }).size(20.0)).frame(false)).clicked() {
                            self.library_visible = !self.library_visible;
                        }
                    });
                });
            });

        // COLLAPSIBLE LIBRARY SIDEBAR
        if self.library_visible {
            egui::SidePanel::right("library_panel")
                .frame(egui::Frame::none().fill(egui::Color32::from_rgb(12, 12, 14)))
                .width_range(250.0..=350.0)
                .show(ctx, |ui| {
                    views::library::render(self, ui);
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_view {
                View::Studio => self.render_dj_studio(ui, &telemetry),
                View::Performance => self.render_performance_mode(ui, &telemetry),
                View::Mixer => views::mixer::render(self, ui, &telemetry),
                View::Sampler => views::sampler::render(self, ui, &telemetry),
                View::Modulation => views::modulation::render(self, ui, &telemetry),
                View::Topology => views::topology::render(self, ui, &telemetry),
                View::Mastering => views::mastering::render(self, ui, &telemetry),
                View::Broadcast => views::broadcast::render(self, ui),
                View::Settings => views::settings::render(self, ui),
                View::Mixer => self.render_mixer(ui, &telemetry),
                View::Sampler => self.render_sampler(ui, &telemetry),
                View::Topology => self.render_topology(ui, &telemetry),
                View::Mastering => self.render_mastering(ui, &telemetry),
                View::Broadcast => self.render_broadcast(ui),
                View::Arranger => self.render_arranger(ui, &telemetry),
            }
        });

        ctx.request_repaint();
    }
}
