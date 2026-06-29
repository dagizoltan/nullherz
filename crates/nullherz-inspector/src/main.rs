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
    Sampler,
    Modulation,
    Mixer,
    Settings,
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
    pub(crate) graph: GraphJson,
    pub(crate) last_telemetry: Arc<Mutex<Option<Telemetry>>>,
    pub(crate) sample_registry: Arc<nullherz_dna::SampleRegistry>,
    pub(crate) command_sender: mpsc::Sender<nullherz_traits::Command>,
    pub(crate) active_view: View,

    // UI State — all controls bound to persistent state
    pub(crate) channel_faders: [f32; 4],
    pub(crate) channel_trims: [f32; 4],
    pub(crate) channel_eq_high: [f32; 4],
    pub(crate) channel_eq_mid: [f32; 4],
    pub(crate) channel_eq_low: [f32; 4],
    pub(crate) channel_fx_slots: [Vec<FxSlot>; 4],
    pub(crate) channel_cue: [bool; 4],
    pub(crate) channel_sync: [bool; 4],
    pub(crate) quantize_enabled: bool,
    pub(crate) master_gain: f32,
    pub(crate) booth_gain: f32,
    pub(crate) rec_gain: f32,
    pub(crate) crossfader_pos: f32,
    pub(crate) library_db: nullherz_dna::LibraryDatabase,
    pub(crate) search_query: String,
    pub(crate) is_streaming: bool,
    pub(crate) active_right_tab: Option<RightTab>,
    pub(crate) selected_deck: usize,
    pub(crate) master_deck: Option<usize>,
    pub(crate) pitch_range: [f32; 4], // 0.08, 0.16, 1.0
    pub(crate) crossfader_curve: f32, // 0.0 = linear, 1.0 = power
    pub(crate) now_playing: [Option<String>; 4],
    pub(crate) global_bpm: f32,
    pub(crate) pitch_bend: [f32; 4],
    pub(crate) macros: [f32; 8],
    pub(crate) macro_names: [String; 8],

    // Peak hold
    pub(crate) channel_peak_hold: [f32; 4],
    pub(crate) master_peak_hold: f32,
    pub(crate) booth_peak_hold: f32,
    pub(crate) rec_peak_hold: f32,

    // Mastering chain state
    pub(crate) mastering_eq_enabled: bool,
    pub(crate) mastering_eq_low: f32,
    pub(crate) mastering_eq_mid: f32,
    pub(crate) mastering_eq_high: f32,
    pub(crate) mastering_comp_enabled: bool,
    pub(crate) mastering_comp_threshold: f32,
    pub(crate) mastering_comp_ratio: f32,
    pub(crate) mastering_comp_attack: f32,
    pub(crate) mastering_limiter_enabled: bool,
    pub(crate) mastering_limiter_gain: f32,
    pub(crate) mastering_limiter_lookahead: f32,

    // Sequencer state
    pub(crate) sequencer_grid: [[bool; 64]; 16],
    pub(crate) sequencer_active_step: usize,

    // Player & Playlist state
    pub(crate) playlists: Vec<Playlist>,
    pub(crate) selected_playlist: Option<usize>,
    pub(crate) player_queue: Vec<Track>,
    pub(crate) player_is_playing: bool,
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
    pub(crate) fn deck_color(i: usize) -> egui::Color32 {
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


<<<<<<< Updated upstream
=======
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



    fn render_signal_stack(&mut self, ui: &mut egui::Ui, _telemetry: &Option<Telemetry>, width: f32) {
        let total_h = 320.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, total_h), egui::Sense::hover());
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(5, 5, 6));
        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(20)));

        let time = ui.input(|i| i.time);
        let deck_h = (total_h - 10.0) / 4.0;
        let playhead_x = rect.center().x;

        for i in 0..4 {
            let deck_color = Self::deck_color(i);
            let deck_rect = egui::Rect::from_min_size(
                rect.min + egui::vec2(5.0, 5.0 + i as f32 * deck_h),
                egui::vec2(width - 10.0, deck_h - 2.0)
            );

            ui.painter().rect_filled(deck_rect, 2.0, egui::Color32::from_rgb(10, 10, 12));

            // Shared playhead vertical line (visual only within deck)
            ui.painter().vline(playhead_x, deck_rect.y_range(), egui::Stroke::new(1.0, egui::Color32::from_gray(30)));

            if let Some(sample) = self.sample_registry.get(i as u64 * 4) {
                let peaks = &sample.metadata.peaks;
                let view_range = 0.05; // 5% of track
                let play_head_norm = (time as f32 % 30.0) / 30.0; // Simulated playhead

                // 1. Phrase Structure (Top 20% of deck height)
                let phrase_h = deck_rect.height() * 0.2;
                let phrase_rect = egui::Rect::from_min_size(deck_rect.min, egui::vec2(deck_rect.width(), phrase_h));
                let block_w = deck_rect.width() / 16.0;
                for b in 0..16 {
                    let bx = phrase_rect.min.x + b as f32 * block_w;
                    let b_rect = egui::Rect::from_min_size(egui::pos2(bx, phrase_rect.min.y), egui::vec2(block_w - 1.0, phrase_h - 1.0));
                    let is_active = (b as f32 / 16.0) < play_head_norm;
                    let fill = if is_active { deck_color.linear_multiply(0.3) } else { egui::Color32::from_gray(20) };
                    ui.painter().rect_filled(b_rect, 1.0, fill);
                }

                // 2. Beat Grid (Middle 20%)
                let grid_h = deck_rect.height() * 0.2;
                let grid_rect = egui::Rect::from_min_size(deck_rect.min + egui::vec2(0.0, phrase_h), egui::vec2(deck_rect.width(), grid_h));
                let total_samples = sample.buffer.len() as f32;
                if total_samples > 0.0 {
                    for &pos in sample.metadata.transients.iter() {
                        let rel_pos = pos as f32 / total_samples;
                        let x_dist = rel_pos - play_head_norm;
                        if x_dist.abs() < view_range / 2.0 {
                            let x_off = (x_dist / view_range) * deck_rect.width();
                            let tx = playhead_x + x_off;
                            if grid_rect.x_range().contains(tx) {
                                ui.painter().circle_filled(egui::pos2(tx, grid_rect.center().y), 2.0, deck_color);
                            }
                        }
                    }
                }

                // 3. Waveform Detail (Bottom 60%)
                let wave_h = deck_rect.height() * 0.6;
                let wave_rect = egui::Rect::from_min_size(deck_rect.min + egui::vec2(0.0, phrase_h + grid_h), egui::vec2(deck_rect.width(), wave_h));

                if !peaks.is_empty() {
                    let mut points = Vec::new();
                    let dw = wave_rect.width();
                    let dh = wave_rect.height();
                    for x in 0..dw as usize {
                        let rel_x = (x as f32 - dw / 2.0) / dw;
                        let track_norm = (play_head_norm + rel_x * view_range).clamp(0.0, 1.0);
                        let p_idx = (track_norm * peaks.len() as f32) as usize;
                        let val = peaks[p_idx.min(peaks.len() - 1)];
                        let y_off = val * (dh / 2.5);
                        points.push(egui::pos2(wave_rect.min.x + x as f32, wave_rect.center().y - y_off));
                        points.push(egui::pos2(wave_rect.min.x + x as f32, wave_rect.center().y + y_off));
                    }
                    ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.0, deck_color.additive())));

                    // Restore Hot Cues
                    for (cue_idx, cue) in sample.metadata.hot_cues.iter().enumerate() {
                        if let Some(pos) = cue {
                            let rel_pos = *pos as f32 / total_samples;
                            let x_dist = rel_pos - play_head_norm;
                            if x_dist.abs() < view_range / 2.0 {
                                let x_off = (x_dist / view_range) * wave_rect.width();
                                let tx = playhead_x + x_off;
                                if wave_rect.x_range().contains(tx) {
                                    ui.painter().vline(tx, wave_rect.y_range(), egui::Stroke::new(1.5, egui::Color32::YELLOW));
                                    ui.painter().text(egui::pos2(tx, wave_rect.min.y + 2.0), egui::Align2::LEFT_TOP, format!("{}", cue_idx+1), egui::FontId::proportional(8.0), egui::Color32::WHITE);
                                }
                            }
                        }
                    }
                }
            } else {
                // Empty state label
                ui.painter().text(deck_rect.center(), egui::Align2::CENTER_CENTER, "NO TRACK LOADED", egui::FontId::proportional(10.0), egui::Color32::from_gray(40));
            }
        }

        // Global Playhead Line across all decks
        ui.painter().vline(playhead_x, rect.y_range(), egui::Stroke::new(1.5, egui::Color32::WHITE.linear_multiply(0.5)));
        ui.painter().text(rect.min + egui::vec2(10.0, 10.0), egui::Align2::LEFT_TOP, "GLOBAL ALIGNMENT TIMELINE", egui::FontId::proportional(11.0), egui::Color32::from_gray(120));
    }

    fn render_deck_controls_row(&mut self, ui: &mut egui::Ui, i: usize, telemetry: &Option<Telemetry>) {
        let color = Self::deck_color(i);
        let is_selected = self.selected_deck == i;

        egui::Frame::none()
            .fill(if is_selected { color.linear_multiply(0.05) } else { egui::Color32::from_rgb(15, 15, 18) })
            .rounding(4.0)
            .stroke(egui::Stroke::new(if is_selected { 2.0 } else { 1.0 }, if is_selected { color } else { egui::Color32::from_gray(30) }))
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // 1. Deck Label & Selection & Master Toggle
                    ui.vertical(|ui| {
                        let (rect, res) = ui.allocate_exact_size(egui::vec2(40.0, 30.0), egui::Sense::click());
                        ui.painter().rect_filled(rect, 2.0, if is_selected { color } else { egui::Color32::from_gray(25) });
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, format!("{}", (b'A' + i as u8) as char), egui::FontId::proportional(16.0), if is_selected { egui::Color32::BLACK } else { color });
                        if res.clicked() { self.selected_deck = i; }

                        let is_master = self.master_deck == Some(i);
                        let m_color = if is_master { color } else { egui::Color32::from_gray(40) };
                        if ui.add(egui::Button::new(egui::RichText::new("MST").small().strong().color(m_color)).frame(true)).clicked() {
                             if is_master { self.master_deck = None; } else { self.master_deck = Some(i); }
                        }
                    });

                    ui.add_space(8.0);

                    // 2. Track Info & Time
                    ui.vertical(|ui| {
                        ui.set_width(200.0);
                        if let Some(ref title) = self.now_playing[i] {
                            ui.label(egui::RichText::new(title).color(color).strong());
                        } else {
                            ui.label(egui::RichText::new("EMPTY").color(egui::Color32::from_gray(60)).strong());
                        }

                        ui.horizontal(|ui| {
                            if let Some(sample) = self.sample_registry.get(i as u64 * 4) {
                                ui.label(egui::RichText::new(format!("{:.1} BPM", sample.metadata.bpm)).small().color(color));
                                if let Some(key) = sample.metadata.root_key {
                                    let notes = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
                                    ui.label(egui::RichText::new(format!("K:{}", notes[(key.round() as usize) % 12])).small().color(color));
                                }
                            }

                            // Restore Track Timing
                            let total_sec = 324.0;
                            let elapsed = (ui.input(|i| i.time) % total_sec as f64) as f32;
                            let remaining = total_sec - elapsed;
                            let mins = (remaining / 60.0) as i32;
                            let secs = (remaining % 60.0) as i32;
                            ui.label(egui::RichText::new(format!("-{:02}:{:02}", mins, secs)).color(color).monospace().size(11.0).strong());
                        });
                    });

                    ui.add_space(10.0);

                    // 3. Transport
                    ui.horizontal(|ui| {
                        let cue_color = if self.channel_cue[i] { egui::Color32::from_rgb(255, 150, 0) } else { egui::Color32::from_gray(40) };
                        if ui.add(egui::Button::new(egui::RichText::new("CUE").color(cue_color).strong()).min_size(egui::vec2(45.0, 28.0))).clicked() {
                            self.channel_cue[i] = !self.channel_cue[i];
                        }

                        let play_color = egui::Color32::from_rgb(0, 255, 100);
                        if ui.add(egui::Button::new(egui::RichText::new("PLAY").color(play_color).strong()).min_size(egui::vec2(50.0, 28.0))).clicked() {
                            // Play toggle
                        }

                        let s_color = if self.channel_sync[i] { color } else { egui::Color32::from_gray(40) };
                        if ui.add(egui::Button::new(egui::RichText::new("SYNC").color(s_color).strong().size(10.0)).min_size(egui::vec2(40.0, 28.0))).clicked() {
                            self.channel_sync[i] = !self.channel_sync[i];
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                target_id: (i as u64 * 4),
                                param_id: 2,
                                value: if self.channel_sync[i] { 1.0 } else { 0.0 },
                                ramp_duration_samples: 0,
                            });
                        }
                    });

                    ui.add_space(15.0);

                    // 4. Pitch
                    ui.horizontal(|ui| {
                        ui.set_width(120.0);
                        let range = 0.92..=1.08;
                        let p_res = widgets::render_horizontal_fader(ui, &mut self.pitch_bend[i], range, color, 80.0, 16.0);
                        if p_res.changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                target_id: (i as u64 * 4),
                                param_id: 1,
                                value: self.pitch_bend[i],
                                ramp_duration_samples: 128,
                            });
                        }
                        let pct = (self.pitch_bend[i] - 1.0) * 100.0;
                        ui.label(egui::RichText::new(format!("{:+.1}%", pct)).size(9.0).monospace().color(color));
                    });

                    // 5. VU Mini
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1].min(1.2));
                        widgets::render_vu_meter(ui, peak, peak, egui::Color32::from_rgb(0, 255, 180), 28.0);
                    });
                });
            });
    }

    fn render_deck_controls_stack(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 4.0);
            for i in 0..4 {
                self.render_deck_controls_row(ui, i, telemetry);
            }
        });
    }

    fn render_dj_studio(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        let total_w = ui.available_width();

        ui.vertical(|ui| {
            ui.add_space(5.0);

            // LAYER 1: MASTER MIX OSCILLOSCOPE (Now integrated at the top)
            self.render_oscillator_monitor(ui, telemetry, total_w);
            ui.add_space(10.0);

            // LAYER 2: GLOBAL ALIGNMENT TIMELINE
            self.render_signal_stack(ui, telemetry, total_w);
            ui.add_space(10.0);

            // LAYER 3: DECK CONTROLS
            self.render_deck_controls_stack(ui, telemetry);
            ui.add_space(15.0);

            // LAYER 4: CENTRAL MIXER
            self.render_central_mixer(ui, telemetry, total_w, 420.0);
        });
    }




    fn render_central_mixer(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>, main_w: f32, height: f32) {
        let rect = ui.allocate_exact_size(egui::vec2(main_w, height), egui::Sense::hover()).0;
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(15, 15, 18));
        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(30)));

        let inner_rect = rect.shrink2(egui::vec2(6.0, 4.0));
        ui.allocate_ui_at_rect(inner_rect, |ui| {
            ui.vertical(|ui| {
                // 1. MASTER CONTROLS STANDALONE ROW (Aligned with Channel Columns)
                ui.add_space(4.0);
                ui.columns(4, |cols| {
                    let labels = ["BOOTH", "REC", "MST"];
                    let colors = [egui::Color32::from_rgb(0, 180, 255), egui::Color32::from_rgb(255, 50, 150), egui::Color32::from_rgb(0, 255, 180)];

                    for i in 0..3 {
                        cols[i].vertical_centered(|ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                                let avail_w = ui.available_width();
                                // Total width estimate: label(25) + space(4) + knob(36) + space(4) + stereo_vu(18) = 87
                                ui.add_space(((avail_w - 87.0) / 2.0).max(0.0));

                                ui.label(egui::RichText::new(labels[i]).size(8.0).strong().color(egui::Color32::from_gray(100)));

                                let (gain, peak_hold, target_id) = match i {
                                    0 => (&mut self.booth_gain, &mut self.booth_peak_hold, 22),
                                    1 => (&mut self.rec_gain, &mut self.rec_peak_hold, 23),
                                    _ => (&mut self.master_gain, &mut self.master_peak_hold, 21),
                                };

                                if widgets::render_knob(ui, gain, 0.0..=1.5, "", colors[i]).changed() {
                                    let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id, param_id: 0, value: *gain, ramp_duration_samples: 128 });
                                }

                                let peak = if i == 2 {
                                    telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2))
                                } else {
                                    telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2)) * (*gain)
                                };
                                if peak > *peak_hold { *peak_hold = peak; } else { *peak_hold *= 0.98; }

                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                                    widgets::render_vu_meter(ui, peak * 0.95, *peak_hold * 0.95, colors[i], 36.0);
                                    widgets::render_vu_meter(ui, peak, *peak_hold, colors[i], 36.0);
                                });
                            });
                        });
                    }
                });
                ui.add_space(8.0);

                // 2. CHANNEL STRIPS
                ui.horizontal_top(|ui| {
                    let col_w = (inner_rect.width() - 12.0) / 4.0;
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);

                    for i in 0..4 {
                        ui.allocate_ui(egui::vec2(col_w, ui.available_height() - 60.0), |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(2.0);

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
                                    let fader_h = 140.0;
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
                                if ui.button("➕").on_hover_text("Add to selected playlist").clicked()
                                    && let Some(idx) = self.selected_playlist {
                                        self.playlists[idx].tracks.push(Track { title: track.title.clone(), artist: track.artist.clone(), bpm: track.metadata.bpm });
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

>>>>>>> Stashed changes

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

                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
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
                        (View::Sampler, "SAMPLER", "🎛"),
                        (View::Modulation, "MOD", "🌊"),
                        (View::Mixer, "MIXER", "🎚"),
                    ] {
                        if self.render_nav_item(ui, icon, label, self.active_view == view, true).clicked() {
                            self.active_view = view;
                        }
                    }
                });

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(4.0);
                    if self.render_nav_item(ui, "⚙", "SETTINGS", self.active_view == View::Settings, true).clicked() {
                        self.active_view = View::Settings;
                    }
                });
            });

        // RIGHT-ALIGNED VERTICAL NAVIGATION (Icon buttons only)
        egui::SidePanel::right("right_nav")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(8, 8, 10)).inner_margin(egui::Margin { left: 8.0, right: 16.0, top: 8.0, bottom: 8.0 }))
            .width_range(68.0..=68.0)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 10.0);

                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("NH").color(egui::Color32::from_gray(40)).strong().size(20.0));
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

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    // Sidebar opener (toggle)
                    let is_open = self.active_right_tab.is_some();
                    if self.render_nav_item(ui, if is_open { "➡" } else { "⬅" }, "TOGGLE SIDEBAR", false, false).clicked() {
                        if is_open { self.active_right_tab = None; } else { self.active_right_tab = Some(RightTab::Library); }
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
                View::Player => views::player::render(self, ui),
                View::Console => views::dj_studio::render(self, ui, &telemetry),
                View::Composer => views::composer::render(self, ui),
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
                View::Sampler => views::sampler::render(self, ui, &telemetry),
                View::Modulation => views::modulation::render(self, ui, &telemetry),
                View::Mixer => views::mixer::render(self, ui, &telemetry),
                View::Settings => views::settings::render(self, ui),
            }
        });

        ctx.request_repaint();
    }
}
