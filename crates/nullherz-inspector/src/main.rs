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
    Metrics,
    Notifications,
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


    fn render_oscillator_monitor(&self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>, width: f32) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 160.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(5, 5, 6));
        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(20)));

        let time = ui.input(|i| i.time);
        let w = rect.width();
        let h = rect.height();

        // High-density background grid
        for i in 0..32 {
            let x = rect.min.x + (i as f32 * (w / 32.0));
            ui.painter().vline(x, rect.y_range(), egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 5)));
        }

        // Master Output Visualization
        let color = egui::Color32::from_rgb(0, 255, 200);
        let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2)); // MASTER target_id: 21
        let drive = self.master_peak_hold.max(peak);

        // Sub-layers for "Glow" effect
        for (idx, glow_color, thickness) in [
            (0, color.linear_multiply(0.15), 6.0),
            (1, color.linear_multiply(0.4), 2.5),
            (2, egui::Color32::WHITE, 1.2),
        ] {
            let points: Vec<egui::Pos2> = (0..w as i32).step_by(1).map(|x| {
                let px = x as f32 / w;
                let wave1 = (px * 35.0 + time as f32 * 14.0 + (idx as f32 * 0.1)).sin();
                let wave2 = (px * 70.0 - time as f32 * 10.0).cos();
                let wave3 = (px * 120.0 + time as f32 * 25.0).sin() * 0.3;
                let amp = (h * 0.4) * (wave1 * 0.5 + wave2 * 0.3 + wave3 * 0.2) * (0.2 + drive * 0.8);
                egui::pos2(rect.min.x + x as f32, rect.center().y + amp)
            }).collect();
            ui.painter().add(egui::Shape::line(points, egui::Stroke::new(thickness, glow_color)));
        }

        ui.painter().text(rect.min + egui::vec2(10.0, 10.0), egui::Align2::LEFT_TOP, "PRECISION MASTER MIX OSCILLOSCOPE", egui::FontId::proportional(11.0), egui::Color32::from_gray(120));
    }



    fn render_dj_studio(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        let total_w = ui.available_width();
        let widget_h = 680.0;

        ui.vertical(|ui| {
            ui.add_space(5.0);

            // ROW 1: FULL WIDTH MASTER OSCILLOSCOPE
            self.render_oscillator_monitor(ui, telemetry, total_w);
            ui.add_space(10.0);

            // ROW 2: PERFORMANCE ROW (2 Deck, Mixer, 2 Deck)
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                let column_w = (total_w - 32.0) / 5.0; // 4 gaps of 8.0px = 32.0

                // DECK A
                ui.allocate_ui(egui::vec2(column_w, widget_h), |ui| { self.render_deck(ui, 0, telemetry, widget_h); });
                // DECK B
                ui.allocate_ui(egui::vec2(column_w, widget_h), |ui| { self.render_deck(ui, 1, telemetry, widget_h); });

                // CENTRAL MIXER
                ui.allocate_ui(egui::vec2(column_w, widget_h), |ui| { self.render_central_mixer(ui, telemetry, column_w, widget_h); });

                // DECK C
                ui.allocate_ui(egui::vec2(column_w, widget_h), |ui| { self.render_deck(ui, 2, telemetry, widget_h); });
                // DECK D
                ui.allocate_ui(egui::vec2(column_w, widget_h), |ui| { self.render_deck(ui, 3, telemetry, widget_h); });
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

        let inner_rect = rect.shrink2(egui::vec2(10.0, 40.0));
        ui.allocate_ui_at_rect(inner_rect, |ui| {
            ui.vertical(|ui| {
                ui.add_space(4.0);

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
                cols[0].with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(5.0);
                    let play_color = egui::Color32::from_rgb(0, 255, 100);
                    let play_btn = egui::Button::new(egui::RichText::new("PLAY").color(play_color).strong())
                        .min_size(egui::vec2(ui.available_width(), 40.0))
                        .rounding(6.0);
                    if ui.add(play_btn).clicked() {
                        // Play logic
                    }
                    ui.add_space(8.0);
                    let cue_color = if self.channel_cue[i] { egui::Color32::from_rgb(255, 150, 0) } else { egui::Color32::from_gray(40) };
                    let cue_btn = egui::Button::new(egui::RichText::new("CUE").color(cue_color).strong())
                        .min_size(egui::vec2(ui.available_width(), 40.0))
                        .rounding(6.0);
                    if ui.add(cue_btn).clicked() {
                        self.channel_cue[i] = !self.channel_cue[i];
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
                    ui.add_space(4.0);
                    let s_color = if self.channel_sync[i] { color } else { egui::Color32::from_gray(40) };
                    let sync_btn = egui::Button::new(egui::RichText::new("SYNC").color(s_color).strong().size(10.0))
                        .min_size(egui::vec2(ui.available_width() * 0.8, 20.0))
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

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        ui.add_space(5.0);
                        let pct = (self.pitch_bend[i] - 1.0) * 100.0;
                        ui.label(egui::RichText::new(format!("{:+.1}%", pct)).size(9.0).monospace().strong().color(color));

                        let range = 0.92..=1.08; // Fixed +-8%
                        let p_res = widgets::render_fader(ui, &mut self.pitch_bend[i], range, color, 80.0, 12.0);
                        if p_res.changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                target_id: (i as u64 * 4),
                                param_id: 1,
                                value: self.pitch_bend[i],
                                ramp_duration_samples: 128,
                            });
                        }
                    });
                });
            });
        });
    });
    }


    fn render_master_strip(&mut self, ui: &mut egui::Ui, i: usize, telemetry: &Option<Telemetry>, col_w: f32) {
        let vu_h = 42.0;
        ui.vertical_centered(|ui| {
            ui.add_space(2.0);
            match i {
                0 => { // BOOTH
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                        let left_pad = (col_w - 36.0 - 18.0) / 2.0; // knob is 36.0, stereo vu is 18.0
                        ui.add_space(left_pad.max(0.0));

                        if widgets::render_knob(ui, &mut self.booth_gain, 0.0..=1.5, "BOOTH", egui::Color32::from_rgb(0, 180, 255)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 22, param_id: 0, value: self.booth_gain, ramp_duration_samples: 128 });
                        }
                        let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2)) * self.booth_gain;
                        if peak > self.booth_peak_hold { self.booth_peak_hold = peak; } else { self.booth_peak_hold *= 0.98; }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                            widgets::render_vu_meter(ui, peak * 0.95, self.booth_peak_hold * 0.95, egui::Color32::from_rgb(0, 180, 255), vu_h);
                            widgets::render_vu_meter(ui, peak, self.booth_peak_hold, egui::Color32::from_rgb(0, 180, 255), vu_h);
                        });
                    });
                }
                1 => { // REC
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                        let left_pad = (col_w - 36.0 - 18.0) / 2.0;
                        ui.add_space(left_pad.max(0.0));

                        if widgets::render_knob(ui, &mut self.rec_gain, 0.0..=1.5, "REC", egui::Color32::from_rgb(255, 50, 150)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 23, param_id: 0, value: self.rec_gain, ramp_duration_samples: 128 });
                        }
                        let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2)) * self.rec_gain;
                        if peak > self.rec_peak_hold { self.rec_peak_hold = peak; } else { self.rec_peak_hold *= 0.98; }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                            widgets::render_vu_meter(ui, peak * 0.95, self.rec_peak_hold * 0.95, egui::Color32::from_rgb(255, 50, 150), vu_h);
                            widgets::render_vu_meter(ui, peak, self.rec_peak_hold, egui::Color32::from_rgb(255, 50, 150), vu_h);
                        });
                    });
                }
                2 => { // MASTER
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                        let left_pad = (col_w - 36.0 - 18.0) / 2.0;
                        ui.add_space(left_pad.max(0.0));

                        if widgets::render_knob(ui, &mut self.master_gain, 0.0..=1.5, "MST", egui::Color32::from_rgb(0, 255, 180)).changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 21, param_id: 0, value: self.master_gain, ramp_duration_samples: 128 });
                        }
                        let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2));
                        if peak > self.master_peak_hold { self.master_peak_hold = peak; } else { self.master_peak_hold *= 0.98; }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                            widgets::render_vu_meter(ui, peak * 0.95, self.master_peak_hold * 0.95, egui::Color32::from_rgb(0, 255, 180), vu_h);
                            widgets::render_vu_meter(ui, peak, self.master_peak_hold, egui::Color32::from_rgb(0, 255, 180), vu_h);
                        });
                    });
                }
                _ => { ui.add_space(vu_h + 8.0); }
            }
        });
    }

    fn render_central_mixer(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>, main_w: f32, height: f32) {
        let rect = ui.allocate_exact_size(egui::vec2(main_w, height), egui::Sense::hover()).0;
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(15, 15, 18));
        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(30)));

        let inner_rect = rect.shrink2(egui::vec2(6.0, 10.0));
        ui.allocate_ui_at_rect(inner_rect, |ui| {
            ui.vertical(|ui| {
                // 1. CHANNEL STRIPS WITH INTEGRATED MASTER CONTROLS
                ui.horizontal_top(|ui| {
                    let col_w = (inner_rect.width() - 12.0) / 4.0;
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);

                    for i in 0..4 {
                        ui.allocate_ui(egui::vec2(col_w, ui.available_height() - 60.0), |ui| {
                            ui.vertical_centered(|ui| {
                                // Master strip sits at the very top of each column
                                self.render_master_strip(ui, i, telemetry, col_w);
                                ui.add_space(8.0);

                                // CHANNEL HEADER
                                egui::Frame::none().fill(Self::deck_color(i).linear_multiply(0.2)).rounding(2.0).inner_margin(4.0).show(ui, |ui| {
                                    ui.set_width(col_w - 4.0);
                                    ui.label(egui::RichText::new(format!("CH{}", i + 1)).small().strong().color(Self::deck_color(i)));
                                });
                                ui.add_space(4.0);

                                // GAIN / TRIM
                                if widgets::render_knob(ui, &mut self.channel_trims[i], 0.0..=2.0, "TRIM", Self::deck_color(i)).changed() {
                                    let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                        target_id: (i as u64 * 4 + 1),
                                        param_id: 0,
                                        value: self.channel_trims[i] * self.channel_faders[i],
                                        ramp_duration_samples: 128,
                                    });
                                }
                                ui.add_space(4.0);

                                // HI / MID / LOW EQ
                                for (label, param_idx, state_val) in [("HI", 2, &mut self.channel_eq_high[i]), ("MID", 1, &mut self.channel_eq_mid[i]), ("LOW", 0, &mut self.channel_eq_low[i])] {
                                    if widgets::render_knob(ui, state_val, 0.0..=2.0, label, Self::deck_color(i)).changed() {
                                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                            target_id: (i as u64 * 4 + 3),
                                            param_id: param_idx,
                                            value: *state_val,
                                            ramp_duration_samples: 0,
                                        });
                                    }
                                    ui.add_space(4.0);
                                }

                                // FADER & VU (Bottom of strip)
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    let fader_h = 180.0;
                                    let fader_w = 20.0;
                                    let spacing = 4.0;
                                    let left_pad = (col_w - fader_w - 8.0 - spacing) / 2.0;
                                    ui.add_space(left_pad.max(0.0));

                                    if widgets::render_fader(ui, &mut self.channel_faders[i], 0.0..=1.0, Self::deck_color(i), fader_h, 24.0).changed() {
                                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                            target_id: (i as u64 * 4 + 1),
                                            param_id: 0,
                                            value: self.channel_trims[i] * self.channel_faders[i],
                                            ramp_duration_samples: 128,
                                        });
                                    }

                                    ui.add_space(spacing);
                                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1].min(1.2));
                                    if peak > self.channel_peak_hold[i] { self.channel_peak_hold[i] = peak; }
                                    else { self.channel_peak_hold[i] *= 0.98; }
                                    widgets::render_vu_meter(ui, peak, self.channel_peak_hold[i], egui::Color32::from_rgb(0, 255, 180), fader_h);
                                });
                            });
                        });
                    }
                });

                ui.add_space(10.0);

                // 3. BOTTOM: GLOBAL CROSSFADER
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(10, 10, 12))
                    .rounding(4.0)
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(30)))
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        ui.set_width(inner_rect.width() - 4.0);
                        ui.vertical_centered(|ui| {
                            ui.horizontal(|ui| {
                                let total_w = ui.available_width();
                                ui.add_space(total_w / 2.0 - 35.0);
                                ui.label(egui::RichText::new("X-FADE").small().strong().color(egui::Color32::from_gray(100)));
                                if ui.add(egui::Button::new(if self.crossfader_curve > 0.5 { "POW" } else { "LIN" }).small()).clicked() {
                                    self.crossfader_curve = if self.crossfader_curve > 0.5 { 0.0 } else { 1.0 };
                                    for target_id in [16, 17] {
                                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id, param_id: 1, value: self.crossfader_curve, ramp_duration_samples: 0 });
                                    }
                                }
                            });
                            ui.add_space(2.0);
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
                        for s_idx in 0..8 {
                            let (rect, res) = ui.allocate_exact_size(egui::vec2(100.0, 32.0), egui::Sense::click());
                            let color = if s_idx == 2 && t == 1 { egui::Color32::from_rgb(0, 255, 100) } else { egui::Color32::from_gray(20) };
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
                        for _s in 0..8 {
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


    fn render_nav_item(&self, ui: &mut egui::Ui, icon: &str, label: &str, is_active: bool, is_left: bool) -> egui::Response {
        let color = if is_active { egui::Color32::from_rgb(0, 255, 200) } else { egui::Color32::from_gray(100) };
        let (rect, res) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::click());

        if is_active {
             ui.painter().rect_filled(rect.expand(2.0), 4.0, color.linear_multiply(0.05));
             let bar_x = if is_left { rect.min.x - 8.0 } else { rect.max.x + 8.0 };
             ui.painter().vline(bar_x, rect.y_range(), egui::Stroke::new(2.0, color));
        }

        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(20.0), color);
        res.on_hover_text(label)
    }
}

impl eframe::App for InspectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let telemetry = *self.last_telemetry.lock().unwrap();

        // LEFT-ALIGNED VERTICAL NAVIGATION
        egui::SidePanel::left("nav_panel")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(8, 8, 10)).inner_margin(8.0))
            .width_range(60.0..=60.0)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 10.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("NH").color(egui::Color32::from_rgb(0, 255, 200)).strong().size(20.0));
                    ui.add_space(10.0);

                    for (view, label, icon) in [
                        (View::Player, "PLAYER", "🎵"),
                        (View::Console, "CONSOLE", "🎚"),
                        (View::Composer, "COMPOSER", "🎹"),
                        (View::Tools, "TOOLS", "🔧"),
                        (View::Mastering, "MASTER", "💎"),
                        (View::Broadcast, "LIVE", "📡"),
                        (View::Topology, "NODES", "🕸"),
                    ] {
                        if self.render_nav_item(ui, icon, label, self.active_view == view, true).clicked() {
                            self.active_view = view;
                        }
                    }

                });
            });

        // RIGHT-ALIGNED VERTICAL NAVIGATION (Icon buttons only)
        egui::SidePanel::right("right_nav")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(8, 8, 10)).inner_margin(8.0))
            .width_range(60.0..=60.0)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 10.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("⚙").color(egui::Color32::from_gray(100)).strong().size(20.0));
                    ui.add_space(10.0);

                    for (tab, label, icon) in [
                        (RightTab::Library, "LIBRARY", "📁"),
                        (RightTab::Metrics, "METRICS", "📊"),
                        (RightTab::Notifications, "NOTIFS", "🔔"),
                    ] {
                        let is_active = self.active_right_tab == Some(tab);
                        if self.render_nav_item(ui, icon, label, is_active, false).clicked() {
                            if is_active { self.active_right_tab = None; } else { self.active_right_tab = Some(tab); }
                        }
                    }
                });
            });

        // RIGHT SIDEBAR CONTENT (Appears to the left of right_nav when active)
        if let Some(tab) = self.active_right_tab {
            egui::SidePanel::right("right_sidebar_content")
                .frame(egui::Frame::none().fill(egui::Color32::from_rgb(12, 12, 14)).inner_margin(egui::Margin::symmetric(12.0, 8.0)))
                .width_range(280.0..=400.0)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.add_space(8.0);
                        match tab {
                            RightTab::Library => views::library::render(self, ui),
                            RightTab::Metrics => views::metrics::render(self, ui),
                            RightTab::Notifications => views::notifications::render(self, ui),
                        }
                    });
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
