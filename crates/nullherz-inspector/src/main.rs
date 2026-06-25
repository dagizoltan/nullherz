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
                .with_inner_size([1280.0, 950.0])
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

#[derive(PartialEq, Clone, Copy)]
enum View {
    Player,
    Console,
    Composer,
    Tools,
    Mastering,
    Broadcast,
    Topology,
}

#[derive(Clone, Default)]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Default)]
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
    channel_fx_slots: [Vec<FxSlot>; 4],
    channel_cue: [bool; 4],
    channel_sync: [bool; 4],
    quantize_enabled: bool,
    master_gain: f32,
    booth_gain: f32,
    rec_gain: f32,
    crossfader_pos: f32,
    library_db: nullherz_dna::LibraryDatabase,
    search_query: String,
    is_streaming: bool,
    active_right_tab: Option<RightTab>,
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
    booth_peak_hold: f32,
    rec_peak_hold: f32,

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

    // Player & Playlist state
    playlists: Vec<Playlist>,
    selected_playlist: Option<usize>,
    player_queue: Vec<Track>,
    player_is_playing: bool,
}

#[derive(Clone, Default)]
pub struct FxSlot {
    pub effect_type: usize,
    pub amount: f32,
    pub enabled: bool,
}

#[derive(PartialEq, Clone, Copy)]
pub enum RightTab {
    Library,
    Settings,
    Account,
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
            active_view: View::Console,
            channel_faders: [0.8; 4],
            channel_trims: [1.0; 4],
            channel_eq_high: [1.0; 4],
            channel_eq_mid: [1.0; 4],
            channel_eq_low: [1.0; 4],
            channel_fx_slots: [
                vec![FxSlot { effect_type: 0, amount: 0.5, enabled: false }, FxSlot { effect_type: 1, amount: 0.2, enabled: false }],
                vec![FxSlot { effect_type: 0, amount: 0.5, enabled: false }, FxSlot { effect_type: 1, amount: 0.2, enabled: false }],
                vec![FxSlot { effect_type: 0, amount: 0.5, enabled: false }, FxSlot { effect_type: 1, amount: 0.2, enabled: false }],
                vec![FxSlot { effect_type: 0, amount: 0.5, enabled: false }, FxSlot { effect_type: 1, amount: 0.2, enabled: false }],
            ],
            channel_cue: [false; 4],
            channel_sync: [false; 4],
            quantize_enabled: true,
            master_gain: 1.0,
            booth_gain: 1.0,
            rec_gain: 1.0,
            crossfader_pos: 0.5,
            library_db: nullherz_dna::LibraryDatabase::load("library.redb").expect("Failed to load library"),
            search_query: String::new(),
            is_streaming: false,
            active_right_tab: Some(RightTab::Library),
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
            booth_peak_hold: 0.0,
            rec_peak_hold: 0.0,
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
            playlists: vec![
                Playlist { name: "Peak Hour".into(), tracks: vec![] },
                Playlist { name: "Deep Tech".into(), tracks: vec![] },
            ],
            selected_playlist: None,
            player_queue: vec![],
            player_is_playing: false,
        }
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
        let widget_h = 500.0;

        ui.vertical(|ui| {
            ui.set_width(total_w);
            ui.add_space(5.0);

            // TOP: OSCILLATOR & MASTER
            ui.horizontal(|ui| {
                let total_w = ui.available_width();
                let master_w = 370.0;
                let oscillator_w = total_w - master_w;

                ui.vertical(|ui| {
                    ui.set_width(oscillator_w);
                    self.render_oscillator_monitor(ui, telemetry);
                });
                ui.add_space(10.0);
                self.render_master_panel(ui, telemetry, 160.0);
            });
            ui.add_space(10.0);

            // MACRO ROW (Hidden)
            /*
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
            */

            // FX RACK ROW (Multi-slot, compact)
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
                for i in 0..4 {
                    if i == 2 { ui.add_space(mixer_w + 10.0); }
                    ui.vertical(|ui| {
                        ui.set_width(deck_w);
                        egui::Frame::none().fill(egui::Color32::from_rgb(10, 10, 12)).rounding(4.0).inner_margin(4.0).show(ui, |ui| {
                            ui.set_width(deck_w - 8.0);
                            egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                                let slot_count = self.channel_fx_slots[i].len();
                                for s_idx in 0..slot_count {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                                        let slot = &mut self.channel_fx_slots[i][s_idx];

                                        ui.checkbox(&mut slot.enabled, "");

                                        egui::ComboBox::from_id_source(format!("fx_sel_{}_{}", i, s_idx))
                                            .selected_text(match slot.effect_type { 0 => "DLY", 1 => "RVB", 2 => "FLG", _ => "OFF" })
                                            .width(60.0)
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut slot.effect_type, 0, "DLY");
                                                ui.selectable_value(&mut slot.effect_type, 1, "RVB");
                                                ui.selectable_value(&mut slot.effect_type, 2, "FLG");
                                            });

                                        widgets::render_knob(ui, &mut slot.amount, 0.0..=1.0, "", Self::deck_color(i));

                                        if ui.button("🗑").clicked() { /* logic to remove */ }
                                    });
                                    if s_idx < slot_count - 1 { ui.separator(); }
                                }
                                if ui.button("+ ADD FX").clicked() {
                                    self.channel_fx_slots[i].push(FxSlot::default());
                                }
                            });
                        });
                    });
                }
            });

            ui.add_space(10.0);

            // PERFORMANCE ROW: A, B, MIXER, C, D
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);

                // DECK A
                ui.vertical(|ui| { ui.set_width(deck_w); self.render_deck(ui, 0, telemetry, widget_h); });
                // DECK B
                ui.vertical(|ui| { ui.set_width(deck_w); self.render_deck(ui, 1, telemetry, widget_h); });

                // ULTRA-COMPACT CENTRAL MIXER
                ui.vertical(|ui| {
                    ui.set_width(mixer_w);
                    self.render_central_mixer(ui, telemetry, mixer_w, widget_h);
                });

                // DECK C
                ui.vertical(|ui| { ui.set_width(deck_w); self.render_deck(ui, 2, telemetry, widget_h); });
                // DECK D
                ui.vertical(|ui| { ui.set_width(deck_w); self.render_deck(ui, 3, telemetry, widget_h); });
            });

            ui.add_space(20.0);

            // INTEGRATED STATUS DASHBOARD
            ui.horizontal(|ui| {
                let dash_w = ui.available_width() - 20.0;
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
                    ui.add_space(20.0);

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

                    ui.add_space(40.0);

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

                    ui.add_space(40.0);

                    // X-RUNS / STABILITY
                    ui.vertical(|ui| {
                        ui.add_space(15.0);
                        let xruns = telemetry.as_ref().map_or(0, |t| t.xrun_count);
                        ui.label(egui::RichText::new("X-RUNS").color(egui::Color32::from_gray(100)).size(9.0).strong());
                        ui.label(egui::RichText::new(format!("{:03}", xruns)).monospace().size(18.0).color(if xruns > 0 { egui::Color32::from_rgb(255, 150, 0) } else { egui::Color32::from_gray(50) }));
                    });
                });
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

        if is_selected {
            ui.painter().rect_filled(rect.expand(2.0), 4.0, color.linear_multiply(0.05));
        }
        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(if is_selected { 3.0 } else { 1.0 }, stroke_color));
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(10, 10, 12));

        ui.child_ui(rect, egui::Layout::top_down(egui::Align::Center)).vertical(|ui| {
            ui.add_space(10.0);

            // DECK INFO HEADER FRAME
            egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 24)).rounding(4.0).inner_margin(4.0).show(ui, |ui| {
                ui.set_width(ui.available_width() - 16.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(deck_name).color(if is_selected { color } else { egui::Color32::from_gray(180) }).strong());

                        let is_master = self.master_deck == Some(i);
                        let master_color = if is_master { color } else { egui::Color32::from_gray(40) };
                        if ui.add(egui::Button::new(egui::RichText::new("MASTER").small().strong().color(master_color)).frame(true)).clicked() {
                            if is_master { self.master_deck = None; } else { self.master_deck = Some(i); }
                        }

                        if let Some(sample) = self.sample_registry.get(i as u64 * 4) {
                            let key_text = if let Some(key) = sample.metadata.root_key {
                                let notes = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
                                format!("{}", notes[(key.round() as usize) % 12])
                            } else {
                                "-".to_string()
                            };
                            ui.add_space(5.0);
                            egui::Frame::none().fill(color.linear_multiply(0.1)).rounding(2.0).inner_margin(2.0).show(ui, |ui| {
                                ui.label(egui::RichText::new(format!(" {} ", key_text)).small().strong().color(color));
                            });
                            ui.label(egui::RichText::new(format!("{:.1} BPM", sample.metadata.bpm)).small().strong().color(color));
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1]);
                            let db = 20.0 * peak.log10().max(-60.0);
                            ui.label(egui::RichText::new(format!("{:.1} dB", db)).small().color(if peak > 1.0 { egui::Color32::RED } else { egui::Color32::from_gray(100) }));
                        });
                    });

                    ui.add_space(2.0);

                    ui.horizontal(|ui| {
                        egui::Frame::none().fill(egui::Color32::from_rgb(5, 5, 6)).inner_margin(6.0).rounding(2.0).show(ui, |ui| {
                            ui.set_width(ui.available_width() - 100.0);
                            if let Some(ref title) = self.now_playing[i] {
                                ui.label(egui::RichText::new(title).color(color).size(13.0).strong());
                            } else {
                                ui.label(egui::RichText::new("READY TO LOAD").color(egui::Color32::from_gray(40)).size(11.0));
                            }
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let total_sec = 324.0;
                            let elapsed = (ui.input(|i| i.time) % total_sec as f64) as f32;
                            let remaining = total_sec - elapsed;
                            let mins = (remaining / 60.0) as i32;
                            let secs = (remaining % 60.0) as i32;
                            ui.label(egui::RichText::new(format!("-{:02}:{:02}", mins, secs)).color(color).monospace().size(18.0).strong());
                        });
                    });
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

            ui.columns(3, |cols| {
                // COL 0: TRANSPORT
                cols[0].vertical_centered(|ui| {
                    ui.add_space(20.0);
                    let cue_color = if self.channel_cue[i] { egui::Color32::from_rgb(255, 150, 0) } else { egui::Color32::from_gray(40) };
                    let cue_btn = egui::Button::new(egui::RichText::new("CUE").color(cue_color).strong())
                        .min_size(egui::vec2(ui.available_width(), 40.0))
                        .rounding(6.0);
                    if ui.add(cue_btn).clicked() {
                        self.channel_cue[i] = !self.channel_cue[i];
                    }
                    ui.add_space(10.0);
                    let play_color = egui::Color32::from_rgb(0, 255, 100);
                    let play_btn = egui::Button::new(egui::RichText::new("PLAY").color(play_color).strong())
                        .min_size(egui::vec2(ui.available_width(), 40.0))
                        .rounding(6.0);
                    if ui.add(play_btn).clicked() {
                        // Play logic
                    }
                });

                // COL 1: JOG
                cols[1].vertical_centered(|ui| {
                    ui.add_space(10.0);
                    let jog_size = (ui.available_width() * 0.95).min(110.0);
                    let (jog_rect, _) = ui.allocate_exact_size(egui::vec2(jog_size, jog_size), egui::Sense::hover());
                    let center = jog_rect.center();
                    let radius = jog_size / 2.0;

                    // Main Platter
                    ui.painter().circle_filled(center, radius, egui::Color32::from_rgb(10, 10, 12));
                    ui.painter().circle_stroke(center, radius, egui::Stroke::new(2.0, egui::Color32::from_gray(40)));

                    // Vinyl Grooves
                    for r in [0.9, 0.8, 0.7, 0.6, 0.5] {
                        ui.painter().circle_stroke(center, radius * r, egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 10)));
                    }

                    // Center Cap
                    ui.painter().circle_filled(center, radius * 0.35, egui::Color32::from_rgb(25, 25, 30));
                    ui.painter().circle_stroke(center, radius * 0.35, egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

                    // Needle / Playhead (Rotating)
                    let angle = (time * 2.0) as f32;
                    let needle_start = center + egui::vec2(angle.cos() * (radius * 0.3), angle.sin() * (radius * 0.3));
                    let needle_end = center + egui::vec2(angle.cos() * (radius * 0.98), angle.sin() * (radius * 0.98));
                    ui.painter().line_segment([needle_start, needle_end], egui::Stroke::new(5.0, color));

                    // Directional Marker
                    ui.painter().circle_filled(needle_end, 5.0, color);
                    ui.painter().circle_stroke(needle_end, 5.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
                });

                // COL 2: PITCH & SYNC
                cols[2].vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let total_w = ui.available_width();
                        let s_color = if self.channel_sync[i] { color } else { egui::Color32::from_gray(40) };
                        let sync_btn = egui::Button::new(egui::RichText::new("SYNC").color(s_color).strong())
                            .min_size(egui::vec2(total_w * 0.4, 24.0))
                            .rounding(4.0);
                        if ui.add(sync_btn).clicked() {
                            self.channel_sync[i] = !self.channel_sync[i];
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                target_id: (i as u64 * 4),
                                param_id: 2,
                                value: if self.channel_sync[i] { 1.0 } else { 0.0 },
                                ramp_duration_samples: 0,
                            });
                        }

                        egui::ComboBox::from_id_source(format!("pitch_range_{}", i))
                            .selected_text(format!("{:.0}%", self.pitch_range[i] * 100.0))
                            .width(total_w * 0.5)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.pitch_range[i], 0.08, "8%");
                                ui.selectable_value(&mut self.pitch_range[i], 0.16, "16%");
                                ui.selectable_value(&mut self.pitch_range[i], 1.0, "WIDE");
                            });
                    });

                    ui.add_space(8.0);
                    let range = (1.0 - self.pitch_range[i])..=(1.0 + self.pitch_range[i]);
                    let p_res = widgets::render_fader(ui, &mut self.pitch_bend[i], range, color, 160.0, 12.0);
                    if p_res.changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: (i as u64 * 4),
                            param_id: 1,
                            value: self.pitch_bend[i],
                            ramp_duration_samples: 128,
                        });
                    }
                    let pct = (self.pitch_bend[i] - 1.0) * 100.0;
                    ui.label(egui::RichText::new(format!("{:+.1}%", pct)).size(9.0).monospace().strong().color(color));
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


    fn render_master_panel(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>, height: f32) {
        let width = 360.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());

        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(15, 15, 18));
        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(40)));

        let inner_rect = rect.shrink(12.0);
        ui.child_ui(inner_rect, egui::Layout::left_to_right(egui::Align::Center)).horizontal(|ui| {
            let inner_h = inner_rect.height() - 20.0;

            // BOOTH CONTROL
            ui.vertical(|ui| {
                ui.set_width(100.0);
                ui.label(egui::RichText::new("BOOTH").size(8.0).strong().color(egui::Color32::from_gray(160)));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                    if widgets::render_fader(ui, &mut self.booth_gain, 0.0..=1.5, egui::Color32::from_rgb(0, 180, 255), inner_h, 15.0).changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 22, param_id: 0, value: self.booth_gain, ramp_duration_samples: 128 });
                    }
                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2)) * self.booth_gain;
                    if peak > self.booth_peak_hold { self.booth_peak_hold = peak; } else { self.booth_peak_hold *= 0.98; }
                    widgets::render_vu_meter(ui, peak * 0.9, self.booth_peak_hold * 0.9, egui::Color32::from_rgb(0, 180, 255), inner_h);
                    widgets::render_vu_meter(ui, peak, self.booth_peak_hold, egui::Color32::from_rgb(0, 180, 255), inner_h);
                });
            });

            ui.add_space(15.0);

            // REC CONTROL
            ui.vertical(|ui| {
                ui.set_width(100.0);
                ui.label(egui::RichText::new("REC").size(8.0).strong().color(egui::Color32::from_gray(160)));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                    if widgets::render_fader(ui, &mut self.rec_gain, 0.0..=1.5, egui::Color32::from_rgb(255, 50, 150), inner_h, 15.0).changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 23, param_id: 0, value: self.rec_gain, ramp_duration_samples: 128 });
                    }
                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2)) * self.rec_gain;
                    if peak > self.rec_peak_hold { self.rec_peak_hold = peak; } else { self.rec_peak_hold *= 0.98; }
                    widgets::render_vu_meter(ui, peak * 0.9, self.rec_peak_hold * 0.9, egui::Color32::from_rgb(255, 50, 150), inner_h);
                    widgets::render_vu_meter(ui, peak, self.rec_peak_hold, egui::Color32::from_rgb(255, 50, 150), inner_h);
                });
            });

            ui.add_space(15.0);

            // MASTER CONTROL
            ui.vertical(|ui| {
                ui.set_width(110.0);
                ui.label(egui::RichText::new("MASTER").size(8.0).strong().color(egui::Color32::from_gray(200)));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                    if widgets::render_fader(ui, &mut self.master_gain, 0.0..=1.5, egui::Color32::from_rgb(0, 255, 180), inner_h, 20.0).changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 21, param_id: 0, value: self.master_gain, ramp_duration_samples: 128 });
                    }
                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2));
                    if peak > self.master_peak_hold { self.master_peak_hold = peak; }
                    else { self.master_peak_hold *= 0.98; }

                    widgets::render_vu_meter(ui, peak * 0.95, self.master_peak_hold * 0.95, egui::Color32::from_rgb(0, 255, 180), inner_h);
                    widgets::render_vu_meter(ui, peak, self.master_peak_hold, egui::Color32::from_rgb(0, 255, 180), inner_h);
                });
            });
        });
    }

    fn render_central_mixer(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>, main_w: f32, height: f32) {
        let rect = ui.allocate_exact_size(egui::vec2(main_w, height), egui::Sense::hover()).0;
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(15, 15, 18));
        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(30)));

        ui.child_ui(rect, egui::Layout::top_down(egui::Align::Center)).vertical_centered(|ui| {
            ui.add_space(10.0);

            // CHANNEL STRIPS
            ui.columns(4, |cols| {
                let col_w = main_w / 4.0;
                for i in 0..4 {
                    cols[i].vertical_centered(|ui| {
                        ui.set_width(col_w);
                        ui.add_space(5.0);

                        // CHANNEL HEADER
                        egui::Frame::none().fill(Self::deck_color(i).linear_multiply(0.2)).rounding(2.0).inner_margin(4.0).show(ui, |ui| {
                            ui.set_width(col_w - 8.0);
                            ui.label(egui::RichText::new(format!("CH {}", i + 1)).strong().color(Self::deck_color(i)));
                        });
                        ui.add_space(8.0);

                        // GAIN / TRIM
                        if widgets::render_knob(ui, &mut self.channel_trims[i], 0.0..=2.0, "TRIM", Self::deck_color(i)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                target_id: (i as u64 * 4 + 1),
                                param_id: 0,
                                value: self.channel_trims[i] * self.channel_faders[i],
                                ramp_duration_samples: 128,
                            });
                        }

                        ui.add_space(12.0);

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
                            ui.add_space(12.0);
                        }

                        ui.add_space(12.0);

                        // FX SUMMARY / MASTER
                        let fx_enabled = self.channel_fx_slots[i].iter().any(|s| s.enabled);
                        let fx_color = if fx_enabled { egui::Color32::from_rgb(0, 255, 200) } else { egui::Color32::from_gray(40) };
                        if ui.add(egui::Button::new(egui::RichText::new("FX").color(fx_color).small().strong()).min_size(egui::vec2(40.0, 24.0))).clicked() {
                            // Toggle first slot for convenience or all? Let's just toggle all for now
                            for slot in &mut self.channel_fx_slots[i] { slot.enabled = !fx_enabled; }
                        }

                        ui.add_space(12.0);

                        // FADER & VU (Precisely Aligned to Center Axis)
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            let strip_w = col_w;
                            let fader_w = 24.0;
                            let spacing = 8.0;

                            // Exact centering math for fader relative to strip
                            let left_pad = (strip_w - fader_w) / 2.0;
                            ui.add_space(left_pad);

                            if widgets::render_fader(ui, &mut self.channel_faders[i], 0.0..=1.0, Self::deck_color(i), 120.0, 30.0).changed() {
                                let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                    target_id: (i as u64 * 4 + 1),
                                    param_id: 0,
                                    value: self.channel_trims[i] * self.channel_faders[i],
                                    ramp_duration_samples: 128,
                                });
                            }

                            ui.add_space(spacing);

                            // VU METER (To the right of fader)
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

            // GLOBAL CROSSFADER
            ui.add_space(10.0);
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(10, 10, 12))
                .rounding(4.0)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(30)))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_width(main_w - 20.0);
                    ui.vertical_centered(|ui| {
                        ui.horizontal(|ui| {
                            let total_w = ui.available_width();
                            ui.add_space(total_w / 2.0 - 35.0);
                            ui.label(egui::RichText::new("X-FADE").small().strong().color(egui::Color32::from_gray(100)));
                            if ui.add(egui::Button::new(if self.crossfader_curve > 0.5 { "POWER" } else { "LIN" }).small()).clicked() {
                                self.crossfader_curve = if self.crossfader_curve > 0.5 { 0.0 } else { 1.0 };
                                for target_id in [16, 17] {
                                    let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id, param_id: 1, value: self.crossfader_curve, ramp_duration_samples: 0 });
                                }
                            }
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("A").color(egui::Color32::from_rgb(0, 200, 255)));
                            let total_w = ui.available_width();
                            let x_res = widgets::render_horizontal_fader(ui, &mut self.crossfader_pos, 0.0..=1.0, egui::Color32::WHITE, total_w - 25.0, 30.0);
                            if x_res.changed() {
                                for target_id in [16, 17] {
                                    let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id, param_id: 0, value: self.crossfader_pos, ramp_duration_samples: 0 });
                                }
                            }
                            ui.label(egui::RichText::new("B").color(egui::Color32::from_rgb(0, 255, 150)));
                        });
                    });
                });
        });
    }


    fn render_player(&mut self, ui: &mut egui::Ui) {
        ui.heading("Precision Hi-Fi Player");
        ui.add_space(12.0);

        ui.horizontal_top(|ui| {
            // Left: Library & Playlists
            ui.vertical(|ui| {
                ui.set_width(320.0);

                ui.strong("Collections");
                ui.add_space(4.0);
                for (idx, pl) in self.playlists.iter().enumerate() {
                    let is_sel = self.selected_playlist == Some(idx);
                    if ui.selectable_label(is_sel, format!("📁 {}", pl.name)).clicked() {
                        self.selected_playlist = Some(idx);
                    }
                }
                if ui.button("+ New Playlist").clicked() {
                    self.playlists.push(Playlist { name: format!("Playlist {}", self.playlists.len() + 1), tracks: vec![] });
                }

                ui.add_space(20.0);
                ui.strong("Quick Access Library");
                ui.separator();
                egui::ScrollArea::vertical().id_source("player_lib").max_height(300.0).show(ui, |ui| {
                    if let Ok(tracks) = self.library_db.list_tracks() {
                        for track in tracks {
                            ui.horizontal(|ui| {
                                if ui.button("➕").on_hover_text("Add to selected playlist").clicked() {
                                    if let Some(idx) = self.selected_playlist {
                                        self.playlists[idx].tracks.push(Track { title: track.title.clone(), artist: track.artist.clone(), bpm: track.metadata.bpm as f32 });
                                    }
                                }
                                ui.label(format!("{} - {}", track.artist, track.title));
                            });
                        }
                    }
                });
            });

            ui.add_space(20.0);

            // Right: Content Area
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 20.0);
                if let Some(idx) = self.selected_playlist {
                    let pl = &mut self.playlists[idx];
                    ui.horizontal(|ui| {
                        ui.heading(&pl.name);
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new(format!("{} tracks", pl.tracks.len())).weak());
                    });
                    ui.separator();
                    ui.add_space(10.0);

                    egui::ScrollArea::vertical().id_source("playlist_tracks").show(ui, |ui| {
                        let mut to_remove = None;
                        for (t_idx, trk) in pl.tracks.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui.button("▶").clicked() {
                                    self.player_is_playing = true;
                                }
                                ui.label(egui::RichText::new(&trk.artist).strong());
                                ui.label("-");
                                ui.label(&trk.title);

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("🗑").clicked() { to_remove = Some(t_idx); }
                                    ui.label(egui::RichText::new(format!("{:.0} BPM", trk.bpm)).weak());
                                });
                            });
                            ui.add_space(2.0);
                        }
                        if let Some(idx) = to_remove { pl.tracks.remove(idx); }
                    });
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(100.0);
                        ui.label(egui::RichText::new("Select a Collection to begin listening").size(18.0).weak());
                    });
                }
            });
        });

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            ui.add_space(20.0);
            egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 24)).inner_margin(12.0).rounding(4.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                   let _ = ui.button(egui::RichText::new("⏮").size(20.0));
                   if ui.button(egui::RichText::new(if self.player_is_playing { "⏸" } else { "▶" }).size(24.0)).clicked() {
                       self.player_is_playing = !self.player_is_playing;
                   }
                   let _ = ui.button(egui::RichText::new("⏭").size(20.0));

                   ui.add_space(20.0);
                   ui.vertical(|ui| {
                       ui.label("Now Playing: -");
                       ui.spacing_mut().slider_width = ui.available_width() - 100.0;
                       ui.add(egui::Slider::new(&mut 0.0, 0.0..=1.0).show_value(false));
                   });
                });
            });
        });
    }

    fn render_composer(&mut self, ui: &mut egui::Ui) {
        ui.heading("Composer - Performance Launcher");
        ui.add_space(12.0);

        egui::ScrollArea::both().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                // Tracks 1-8
                for t in 0..8 {
                    ui.vertical(|ui| {
                        ui.set_width(100.0);

                        // TRACK HEADER
                        egui::Frame::none().fill(egui::Color32::from_gray(25)).rounding(2.0).inner_margin(4.0).show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(egui::RichText::new(format!("TRACK {}", t+1)).small().strong());
                                ui.horizontal(|ui| {
                                    let _ = ui.button(egui::RichText::new("S").small());
                                    let _ = ui.button(egui::RichText::new("M").small());
                                });
                                widgets::render_vu_meter(ui, 0.0, 0.0, egui::Color32::from_rgb(0, 255, 100), 60.0);
                            });
                        });

                        ui.add_space(8.0);

                        // CLIPS
                        for s in 0..8 {
                            let (rect, res) = ui.allocate_exact_size(egui::vec2(100.0, 32.0), egui::Sense::click());
                            let color = if s == 2 && t == 1 { egui::Color32::from_rgb(0, 255, 100) } else { egui::Color32::from_gray(20) };
                            ui.painter().rect_filled(rect, 2.0, color);
                            ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, egui::Color32::from_gray(40)));

                            if res.clicked() { /* Trigger clip */ }
                        }

                        ui.add_space(10.0);
                        // VOLUME FADER
                        let mut vol = 0.8;
                        widgets::render_fader(ui, &mut vol, 0.0..=1.0, egui::Color32::from_gray(100), 100.0, 15.0);
                    });
                    ui.add_space(8.0);
                }

                ui.separator();

                // MASTER / SCENE LAUNCHER
                ui.vertical(|ui| {
                    ui.set_width(60.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("MASTER").small().strong());
                        ui.add_space(76.0); // Align with clip top
                        for s in 0..8 {
                            if ui.add(egui::Button::new(egui::RichText::new("▶").small()).min_size(egui::vec2(50.0, 32.0))).clicked() {
                                // Launch scene
                            }
                            ui.add_space(4.0);
                        }
                    });
                });
            });
        });
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
                let _ = ui.button("Trigger Pattern A on Deck 1 at Beat 16");
                let _ = ui.button("Set Macro 1 to 0.8 at Beat 32");
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
                        (View::Player, "PLAYER", "🎵"),
                        (View::Console, "CONSOLE", "🎚"),
                        (View::Composer, "COMPOSER", "🎹"),
                        (View::Tools, "TOOLS", "🔧"),
                        (View::Mastering, "MASTER", "💎"),
                        (View::Broadcast, "LIVE", "📡"),
                        (View::Topology, "NODES", "🕸"),
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

                        // RIGHT SIDEBAR / LIBRARY TOGGLE
                        let lib_active = self.active_right_tab == Some(RightTab::Library);
                        let lib_color = if lib_active { egui::Color32::from_rgb(0, 255, 200) } else { egui::Color32::from_gray(100) };
                        if ui.add(egui::Button::new(egui::RichText::new("📁").size(20.0)).frame(false)).clicked() {
                            self.active_right_tab = if lib_active { None } else { Some(RightTab::Library) };
                        }
                        ui.add_space(10.0);

                        for (tab, label, icon) in [
                            (RightTab::Account, "ACCOUNT", "👤"),
                            (RightTab::Settings, "SETTINGS", "⚙"),
                        ] {
                             let is_active = self.active_right_tab == Some(tab);
                             let color = if is_active { egui::Color32::from_rgb(0, 255, 200) } else { egui::Color32::from_gray(100) };
                             let (rect, res) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::click());
                             if res.clicked() {
                                 if is_active { self.active_right_tab = None; }
                                 else { self.active_right_tab = Some(tab); }
                             }
                             ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(20.0), color);
                             res.on_hover_text(label);
                             ui.add_space(10.0);
                        }
                    });
                });
            });

        if let Some(tab) = self.active_right_tab {
            egui::SidePanel::right("right_sidebar")
                .frame(egui::Frame::none().fill(egui::Color32::from_rgb(12, 12, 14)).inner_margin(12.0))
                .width_range(280.0..=400.0)
                .show(ctx, |ui| {
                    match tab {
                        RightTab::Library => views::library::render(self, ui),
                        RightTab::Settings => views::settings::render(self, ui),
                        RightTab::Account => { ui.heading("Account"); ui.label("User Profile & Statistics"); },
                    }
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_view {
                View::Player => self.render_player(ui),
                View::Console => self.render_dj_studio(ui, &telemetry),
                View::Composer => self.render_composer(ui),
                View::Tools => {
                    ui.heading("Precision Audio Tools");
                    ui.add_space(20.0);
                    ui.columns(3, |cols| {
                        cols[0].group(|ui| {
                            ui.strong("Instrument Tuner");
                            ui.label("Cents: +1.2");
                            ui.label("Frequency: 441.2 Hz");
                            ui.add(egui::ProgressBar::new(0.51).text("IN TUNE"));
                        });
                        cols[1].group(|ui| {
                            ui.strong("Sample Editor");
                            ui.label("Crop & Normalize");
                            let _ = ui.button("✂ CROP");
                            let _ = ui.button("🔊 NORMALIZE");
                        });
                        cols[2].group(|ui| {
                            ui.strong("File Inspector");
                            ui.label("WAV / 24-bit / 48kHz");
                            let _ = ui.button("Metadata");
                        });
                    });
                },
                View::Mastering => views::mastering::render(self, ui, &telemetry),
                View::Broadcast => views::broadcast::render(self, ui),
                View::Topology => views::topology::render(self, ui, &telemetry),
            }
        });

        ctx.request_repaint();
    }
}
