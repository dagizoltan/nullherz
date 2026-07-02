use std::fs;
use std::env;
use serde::{Deserialize, Serialize};
use eframe::egui;
use std::sync::{Arc, Mutex};
use audio_core::Telemetry;
use nullherz_traits::{Command, CoreCommand, MixerCommand, PerformanceCommand, ResourceCommand, OpaqueEnvelope};
use std::sync::mpsc;

mod widgets;
mod views;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeJson {
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EdgeJson {
    pub from: u32,
    pub to: u32,
    pub input_idx: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphJson {
    pub nodes: Vec<NodeJson>,
    pub edges: Vec<EdgeJson>,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum View {
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
    Library,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum RightTab {
    Library,
    Metrics,
    Notifications,
}

pub struct FxSlot {
    pub effect_type: u32,
    pub amount: f32,
    pub enabled: bool,
}

pub struct Track {
    pub title: String,
    pub artist: String,
    pub bpm: f32,
}

pub struct Playlist {
    pub name: String,
    pub tracks: Vec<Track>,
}

pub struct InspectorApp {
    pub(crate) graph: GraphJson,
    pub(crate) command_sender: mpsc::Sender<Command>,
    pub(crate) last_telemetry: Arc<Mutex<Option<Telemetry>>>,
    pub(crate) active_view: View,
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
    pub(crate) pitch_range: [f32; 4],
    pub(crate) crossfader_curve: f32,
    pub(crate) now_playing: [Option<String>; 4],
    pub(crate) global_bpm: f32,
    pub(crate) pitch_bend: [f32; 4],
    pub(crate) macros: [f32; 8],
    pub(crate) macro_names: [String; 8],
    pub(crate) channel_peak_hold: [f32; 4],
    pub(crate) master_peak_hold: f32,
    pub(crate) booth_peak_hold: f32,
    pub(crate) rec_peak_hold: f32,
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
    pub(crate) spectral_window_shape: u32,
    pub(crate) sequencer_grid: [[bool; 64]; 16],
    pub(crate) sequencer_active_step: usize,
    pub(crate) sampler_slicer_mode: bool,
    pub(crate) sampler_slice_grid: f32,
    pub(crate) sampler_beats_per_bar: f32,
    pub(crate) playlists: Vec<Playlist>,
    pub(crate) selected_playlist: Option<usize>,
    pub(crate) player_queue: Vec<Track>,
    pub(crate) player_is_playing: bool,
}

impl InspectorApp {
    pub fn new(graph: GraphJson, cc: &eframe::CreationContext<'_>) -> Self {
        let (cmd_tx, _cmd_rx) = mpsc::channel::<Command>();
        Self {
            graph,
            command_sender: cmd_tx,
            last_telemetry: Arc::new(Mutex::new(None)),
            active_view: View::Console,
            channel_faders: [1.0; 4],
            channel_trims: [1.0; 4],
            channel_eq_high: [1.0; 4],
            channel_eq_mid: [1.0; 4],
            channel_eq_low: [1.0; 4],
            channel_fx_slots: [vec![], vec![], vec![], vec![]],
            channel_cue: [false; 4],
            channel_sync: [false; 4],
            quantize_enabled: true,
            master_gain: 1.0,
            booth_gain: 1.0,
            rec_gain: 1.0,
            crossfader_pos: 0.5,
            library_db: nullherz_dna::LibraryDatabase::load("library.redb").unwrap(),
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
            macro_names: std::array::from_fn(|i| format!("MACRO {}", i + 1)),
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
            playlists: vec![],
            selected_playlist: None,
            player_queue: vec![],
            player_is_playing: false,
        }
    }

    pub fn deck_color(i: usize) -> egui::Color32 {
        match i {
            0 => egui::Color32::from_rgb(0, 255, 200),
            1 => egui::Color32::from_rgb(0, 150, 255),
            2 => egui::Color32::from_rgb(255, 100, 0),
            3 => egui::Color32::from_rgb(255, 0, 100),
            _ => egui::Color32::WHITE,
        }
    }
}

impl eframe::App for InspectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let telemetry = self.last_telemetry.lock().unwrap().clone();

        egui::CentralPanel::default().show(ctx, |ui| {
             ui.heading("nullherz Studio");
             match self.active_view {
                 View::Console => views::dj_studio::render(self, ui, &telemetry),
                 View::Sampler => views::sampler::render(self, ui, &telemetry),
                 View::Mixer => views::mixer::render(self, ui, &telemetry),
                 View::Library => views::library::render(self, ui),
                 _ => { ui.label("View coming soon..."); }
             }
        });
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "nullherz Studio",
        native_options,
        Box::new(|cc| {
            let graph = GraphJson { nodes: vec![], edges: vec![] };
            Box::new(InspectorApp::new(graph, cc))
        }),
    )
}
