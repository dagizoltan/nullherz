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

    // Spectral Morph extension state
    pub(crate) spectral_window_shape: u8,

    // Sequencer state
    pub(crate) sequencer_grid: [[bool; 64]; 16],
    pub(crate) sequencer_active_step: usize,

    // Sampler/Slicer state
    pub(crate) sampler_slicer_mode: bool,
    pub(crate) sampler_slice_grid: f32, // index into [1.0, 0.5, 0.25]
    pub(crate) sampler_beats_per_bar: f32,

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
            spectral_window_shape: 0,
            sequencer_grid: [[false; 64]; 16],
            sequencer_active_step: 0,
            sampler_slicer_mode: false,
            sampler_slice_grid: 0.25,
            sampler_beats_per_bar: 4.0,
            playlists: vec![
                Playlist { name: "Peak Hour".into(), tracks: vec![] },
                Playlist { name: "Deep Tech".into(), tracks: vec![] },
            ],
            selected_playlist: None,
            player_queue: vec![],
            player_is_playing: false,
        }
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
