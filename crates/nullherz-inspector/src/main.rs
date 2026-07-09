use serde::{Deserialize, Serialize};
use eframe::egui;
use std::sync::{Arc, Mutex};
use audio_core::Telemetry;
use nullherz_traits::Command;
use std::sync::mpsc;

mod widgets;
mod views;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeJson {
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
    pub name: String,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EdgeJson {
    pub from: u32,
    pub to: u32,
    pub input_idx: u32,
    #[serde(default)]
    pub output_idx: u32,
    #[serde(default)]
    pub buffer_idx: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphJson {
    pub nodes: Vec<NodeJson>,
    pub edges: Vec<EdgeJson>,
    pub node_assignments: nullherz_traits::NodeAssignmentArray,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum View {
    Player,
    Console,
    Composer,
    Editor,
    Sampler,
    Breeder,
    Broadcast,
    Topology,
    Account,
    Settings,
    // Secondary/Legacy Views
    Tools,
    Mastering,
    Modulation,
    Mixer,
    Library,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum RightTab {
    Library,
    Metrics,
    Notifications,
    GeneticCloud,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum SettingsTab {
    General,
    Audio,
    Midi,
    Network,
    Calibration,
}


pub struct Track {
    pub title: String,
    pub artist: String,
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
    pub(crate) channel_eq_high: [f32; 4],
    pub(crate) channel_eq_mid: [f32; 4],
    pub(crate) channel_eq_low: [f32; 4],
    pub(crate) channel_filter: [f32; 4],
    pub(crate) channel_personality_metallic: [f32; 4],
    pub(crate) channel_personality_organic: [f32; 4],
    pub(crate) channel_personality_warm: [f32; 4],
    pub(crate) channel_personality_aggressive: [f32; 4],
    pub(crate) channel_sync: [bool; 4],
    pub(crate) quantize_enabled: bool,
    pub(crate) master_gain: f32,
    pub(crate) crossfader_pos: f32,
    pub(crate) library_db: nullherz_dna::LibraryDatabase,
    pub(crate) active_crate: Option<String>,
    pub(crate) search_query: String,
    pub(crate) is_streaming: bool,
    pub(crate) active_right_tab: Option<RightTab>,
    pub(crate) master_deck: Option<usize>,
    pub(crate) now_playing: [Option<u64>; 4],
    pub(crate) global_bpm: f32,
    pub(crate) macros: [f32; 8],
    pub(crate) _macro_names: [String; 8],
    pub(crate) channel_peak_hold: [f32; 4],
    pub(crate) master_peak_hold: f32,
    pub(crate) _booth_peak_hold: f32,
    pub(crate) _rec_peak_hold: f32,
    pub(crate) mastering_eq_enabled: bool,
    pub(crate) mastering_eq_low: f32,
    pub(crate) mastering_eq_mid: f32,
    pub(crate) mastering_eq_high: f32,
    pub(crate) spectral_window_shape: u32,
    pub(crate) sequencer_grid: [[f32; 64]; 16],
    pub(crate) sequencer_active_step: usize,
    pub(crate) sampler_slicer_mode: bool,
    pub(crate) _playlists: Vec<Playlist>,
    pub(crate) player_is_playing: bool,
    pub(crate) cached_library: Vec<nullherz_dna::LibraryTrack>,
    pub(crate) library_needs_refresh: bool,
    pub(crate) breeding_view: views::breeder::BreederView,
    pub(crate) wgpu_renderer: Option<Arc<Mutex<nullherz_ui_hal::render::wgpu_backend::WgpuRenderer>>>,
    pub(crate) waveform_renderer: Option<Arc<Mutex<nullherz_ui_hal::render::waveform_renderer::WaveformRenderer>>>,
    pub(crate) active_connection_source: Option<(u32, u32)>, // (node_idx, output_idx)
    pub(crate) active_node_drag: Option<u32>,
    pub(crate) smart_crate_builder_open: bool,
    pub(crate) smart_crate_def: nullherz_dna::SmartCrateDefinition,
    pub(crate) visualizer_damping: f32,
    pub(crate) damped_spectrum: [f32; 128],
    pub(crate) damped_goniometer: [f32; 128],
    pub(crate) damped_latent: [f32; 16],
    pub(crate) damped_peaks: [f32; 4],
    pub(crate) damped_master_peaks: [f32; 2],
    pub(crate) discovered_sidecars: Vec<nullherz_traits::SidecarManifest>,
    pub(crate) personality_macro_mode: bool,
    pub(crate) focused_deck: usize,
    pub(crate) track_mutes: [bool; 16],
    pub(crate) track_solos: [bool; 16],
    pub(crate) record_automation: bool,
    pub(crate) _automation_data: std::collections::HashMap<u64, Vec<(f64, f32)>>, // target_id -> [(beat, value)]
    pub(crate) sampler_waveform_zoom: f32,
    pub(crate) active_settings_tab: SettingsTab,
    pub(crate) sampler_input_gain: f32,
    pub(crate) sampler_monitor_level: f32,
    pub(crate) sampler_is_recording: bool,
    pub(crate) sampler_is_stereo: bool,
    pub(crate) sampler_input_source: usize, // 0: Master, 1-4: Decks, 5: External
    pub(crate) selected_library_track: Option<u64>,
    pub(crate) bypassed_nodes: std::collections::HashSet<u32>,
    pub(crate) theme: nullherz_ui_hal::Theme,
    pub(crate) last_update_time: f64,
    pub(crate) ingestion_path: String,
    pub(crate) node_map: std::collections::HashMap<String, u32>,
    pub(crate) playlist_queue: std::collections::VecDeque<u64>,
    pub(crate) evolution_strengths: [f32; 16],
    pub(crate) next_sample_id: u64,
    pub(crate) editor_selection: Option<(f32, f32)>, // normalized (start, end)
    pub(crate) audio_devices: Vec<String>,
    pub(crate) selected_audio_device: String,
}

impl InspectorApp {
    pub fn get_node_id(&self, name: &str) -> u32 {
        *self.node_map.get(name).unwrap_or(&0)
    }

    pub fn new(graph: GraphJson, _cc: &eframe::CreationContext<'_>) -> Self {
        let (cmd_tx, _cmd_rx) = mpsc::channel::<Command>();
        Self {
            graph,
            command_sender: cmd_tx,
            last_telemetry: Arc::new(Mutex::new(None)),
            active_view: View::Console,
            channel_faders: [1.0; 4],
            channel_eq_high: [1.0; 4],
            channel_eq_mid: [1.0; 4],
            channel_eq_low: [1.0; 4],
            channel_filter: [0.5; 4],
            channel_personality_metallic: [0.0; 4],
            channel_personality_organic: [0.0; 4],
            channel_personality_warm: [0.0; 4],
            channel_personality_aggressive: [0.0; 4],
            channel_sync: [false; 4],
            quantize_enabled: true,
            master_gain: 1.0,
            crossfader_pos: 0.5,
            library_db: nullherz_dna::LibraryDatabase::load("library.redb").unwrap_or_else(|e| {
                eprintln!("Warning: Failed to load library.redb ({}). Using transient storage.", e);
                nullherz_dna::LibraryDatabase::load(":memory:").expect("Failed to initialize transient LibraryDatabase")
            }),
            active_crate: None,
            search_query: String::new(),
            is_streaming: false,
            active_right_tab: Some(RightTab::Library),
            master_deck: Some(0),
            now_playing: [None, None, None, None],
            global_bpm: 128.0,
            macros: [0.0; 8],
            _macro_names: std::array::from_fn(|i| format!("MACRO {}", i + 1)),
            channel_peak_hold: [0.0; 4],
            master_peak_hold: 0.0,
            _booth_peak_hold: 0.0,
            _rec_peak_hold: 0.0,
            mastering_eq_enabled: true,
            mastering_eq_low: 1.0,
            mastering_eq_mid: 1.0,
            mastering_eq_high: 1.0,
            spectral_window_shape: 0,
            sequencer_grid: [[0.0; 64]; 16],
            sequencer_active_step: 0,
            sampler_slicer_mode: false,
            _playlists: vec![],
            player_is_playing: false,
            cached_library: vec![],
            library_needs_refresh: true,
            breeding_view: views::breeder::BreederView::new(),
            wgpu_renderer: None,
            waveform_renderer: None,
            active_connection_source: None,
            active_node_drag: None,
            smart_crate_builder_open: false,
            smart_crate_def: nullherz_dna::SmartCrateDefinition {
                name: "New Smart Crate".into(),
                target_dna: None,
                threshold: 0.5,
                spectral_tilt_range: None,
                rhythmic_syncopation_range: None,
                glitch_density_range: None,
                genre: None,
                bpm_range: None,
                energy_range: None,
                root_key: None,
            },
            visualizer_damping: 0.1,
            damped_spectrum: [0.0; 128],
            damped_goniometer: [0.0; 128],
            damped_latent: [0.0; 16],
            damped_peaks: [0.0; 4],
            damped_master_peaks: [0.0; 2],
            discovered_sidecars: vec![],
            personality_macro_mode: false,
            focused_deck: 0,
            track_mutes: [false; 16],
            track_solos: [false; 16],
            record_automation: false,
            _automation_data: std::collections::HashMap::new(),
            sampler_waveform_zoom: 1.0,
            active_settings_tab: SettingsTab::General,
            sampler_input_gain: 1.0,
            sampler_monitor_level: 0.0,
            sampler_is_recording: false,
            sampler_is_stereo: true,
            sampler_input_source: 0,
            selected_library_track: None,
            bypassed_nodes: std::collections::HashSet::new(),
            theme: nullherz_ui_hal::Theme {
                accent: egui::Color32::from_rgb(0, 255, 200), // #00FFC8-ish
                bg_dark: egui::Color32::from_rgb(10, 10, 12), // #0A0A0C
                bg_med: egui::Color32::from_rgb(30, 30, 30),  // #1E1E1E
                text_primary: egui::Color32::WHITE,
                socket_color: egui::Color32::from_gray(80),
            },
            last_update_time: 0.0,
            ingestion_path: "tracks/".to_string(),
            playlist_queue: std::collections::VecDeque::new(),
            evolution_strengths: [0.0; 16],
            next_sample_id: 1000,
            editor_selection: None,
            audio_devices: vec!["default".to_string()],
            selected_audio_device: "default".to_string(),
            node_map: [
                ("deck_a_sampler".to_string(), 0), ("deck_a_gain".to_string(), 4), ("deck_a_filter".to_string(), 3),
                ("deck_b_sampler".to_string(), 4), ("deck_b_gain".to_string(), 8), ("deck_b_filter".to_string(), 7),
                ("deck_c_sampler".to_string(), 8), ("deck_c_gain".to_string(), 12), ("deck_c_filter".to_string(), 11),
                ("deck_d_sampler".to_string(), 12), ("deck_d_gain".to_string(), 16), ("deck_d_filter".to_string(), 15),
                ("master_sum".to_string(), 30), ("master_crossfader".to_string(), 20), ("master_limiter".to_string(), 35),
                ("capture_node".to_string(), 110), ("sequencer_node".to_string(), 70), ("sampler_node".to_string(), 100),
            ].into_iter().collect(),
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
        let current_time = ctx.input(|i| i.time);
        let is_focused = ctx.input(|i| i.focused);

        // Background Throttling: Skip telemetry processing if unfocused and updated recently (<100ms)
        let should_process = is_focused || (current_time - self.last_update_time) > 0.1;

        let telemetry = if should_process {
            self.last_update_time = current_time;
            self.last_telemetry.lock().ok().and_then(|t| t.clone())
        } else {
            None
        };

        // Update Damping (Liquid Asymmetrical Damping: Fast Attack, Slow Decay)
        if let Some(ref t) = telemetry {
            // Synchronize Master Deck from Telemetry
            self.master_deck = Some((t.active_master_deck as u8 - b'A') as usize);

            let d = self.visualizer_damping.clamp(0.01, 1.0);
            let decay = d * 0.5; // Slower decay for "liquid" feel

            // Optimized damping using Lerp formula: current + (target - current) * alpha
            for i in 0..128 {
                let target_spec = t.spectrum[i];
                let alpha = if target_spec > self.damped_spectrum[i] { d } else { decay };
                self.damped_spectrum[i] += (target_spec - self.damped_spectrum[i]) * alpha;

                let target_gonio = t.goniometer_pts[i];
                let alpha_g = if target_gonio.abs() > self.damped_goniometer[i].abs() { d } else { decay };
                self.damped_goniometer[i] += (target_gonio - self.damped_goniometer[i]) * alpha_g;
            }
            for i in 0..16 {
                let target_latent = t.dna_latent_space[i];
                let alpha_l = if target_latent.abs() > self.damped_latent[i].abs() { d } else { decay };
                self.damped_latent[i] += (target_latent - self.damped_latent[i]) * alpha_l;
            }
            for i in 0..4 {
                let target_peak = t.peak_levels[i];
                let alpha_p = if target_peak > self.damped_peaks[i] { 1.0 } else { decay * 0.5 };
                self.damped_peaks[i] += (target_peak - self.damped_peaks[i]) * alpha_p;
            }
            for i in 0..2 {
                let target_m_peak = t.peak_levels[i];
                let alpha_mp = if target_m_peak > self.damped_master_peaks[i] { 1.0 } else { decay * 0.5 };
                self.damped_master_peaks[i] += (target_m_peak - self.damped_master_peaks[i]) * alpha_mp;
            }

            // Sync node map from telemetry
            for i in 0..32 {
                let key_bytes = t.node_map_keys[i];
                if key_bytes[0] != 0 {
                    let name = String::from_utf8_lossy(&key_bytes).trim_matches(char::from(0)).to_string();
                    self.node_map.insert(name, t.node_map_values[i]);
                }
            }

            // Sync audio devices from telemetry
            let mut devs = Vec::new();
            for i in 0..16 {
                let dev_bytes = t.audio_devices[i].name;
                if dev_bytes[0] != 0 {
                    devs.push(String::from_utf8_lossy(&dev_bytes).trim_matches(char::from(0)).to_string());
                }
            }
            if !devs.is_empty() {
                self.audio_devices = devs;
            }
        }

        // 1. Left Sidebar (Navigation Plane)
        egui::SidePanel::left("left_sidebar")
            .resizable(false)
            .default_width(70.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    // Minimalist Logo/Brand
                    ui.label(egui::RichText::new("Ω").size(24.0).color(egui::Color32::from_rgb(0, 255, 200)));
                    ui.add_space(20.0);

                    let top_nav = [
                        (View::Player, "💿", "MEDIA PLAYER"),
                        (View::Console, "📻", "DJ CONSOLE"),
                        (View::Composer, "🎹", "COMPOSER"),
                        (View::Editor, "✂", "EDITOR"),
                        (View::Sampler, "🎤", "SAMPLER"),
                        (View::Breeder, "🧬", "DNA BREEDER"),
                        (View::Broadcast, "📡", "BROADCAST"),
                    ];

                    let bottom_nav = [
                        (View::Topology, "🕸", "TOPOLOGY"),
                        (View::Account, "👤", "ACCOUNT"),
                        (View::Settings, "⚙", "SETTINGS"),
                    ];

                    // Bottom Navigation pinned to bottom
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        for (view, icon, label) in bottom_nav.into_iter().rev() {
                            let is_selected = self.active_view == view;
                            let bg_color = if is_selected { egui::Color32::from_gray(50) } else { egui::Color32::TRANSPARENT };

                            if ui.add(egui::Button::new(egui::RichText::new(icon).size(20.0)).fill(bg_color).min_size(egui::vec2(50.0, 50.0))).on_hover_text(label).clicked() {
                                self.active_view = view;
                            }
                            ui.add_space(10.0);
                        }

                        ui.separator();

                        // Top Navigation in a scroll area to take remaining space
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            egui::ScrollArea::vertical().id_source("nav_scroll").show(ui, |ui| {
                                for (view, icon, label) in top_nav {
                                    let is_selected = self.active_view == view;
                                    let bg_color = if is_selected { egui::Color32::from_gray(50) } else { egui::Color32::TRANSPARENT };

                                    if ui.add(egui::Button::new(egui::RichText::new(icon).size(20.0)).fill(bg_color).min_size(egui::vec2(50.0, 50.0))).on_hover_text(label).clicked() {
                                        self.active_view = view;
                                    }
                                    ui.add_space(10.0);
                                }
                            });
                        });
                    });
                });
            });

        // 2. Right Sidebar (Intelligence Plane - Collapsible)
        if let Some(tab) = self.active_right_tab {
            egui::SidePanel::right("right_sidebar")
                .resizable(true)
                .default_width(300.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.selectable_label(self.active_right_tab == Some(RightTab::Library), "LIBRARY").clicked() {
                            self.active_right_tab = Some(RightTab::Library);
                        }
                        if ui.selectable_label(self.active_right_tab == Some(RightTab::GeneticCloud), "CLOUD").clicked() {
                            self.active_right_tab = Some(RightTab::GeneticCloud);
                        }
                        if ui.selectable_label(self.active_right_tab == Some(RightTab::Notifications), "AI & INSIGHTS").clicked() {
                            self.active_right_tab = Some(RightTab::Notifications);
                        }
                        if ui.selectable_label(self.active_right_tab == Some(RightTab::Metrics), "SYSTEM").clicked() {
                            self.active_right_tab = Some(RightTab::Metrics);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("❌").clicked() { self.active_right_tab = None; }
                        });
                    });
                    ui.separator();

                    match tab {
                        RightTab::Library => views::library::render(self, ui),
                        RightTab::GeneticCloud => views::genetic_cloud::render(self, ui),
                        RightTab::Notifications => views::notifications::render(self, ui),
                        RightTab::Metrics => views::metrics::render(self, ui),
                    }
                });
        }

        // 3. Bottom Bar (Status & Global Controls)
        egui::TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("nullherz Alpha").size(10.0).color(egui::Color32::from_gray(100)));
                ui.separator();

                if let Some(t) = &telemetry {
                    ui.label(format!("BPM: {:.1}", t.bpm));
                    ui.separator();
                    ui.label(format!("POS: {:.2}", t.beat_position));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Right Sidebar Tab Buttons
                    let tabs = [
                        (RightTab::Library, "📂", "LIBRARY"),
                        (RightTab::GeneticCloud, "☁", "GENETIC CLOUD"),
                        (RightTab::Notifications, "🧠", "AI & INSIGHTS"),
                        (RightTab::Metrics, "📊", "METRICS"),
                    ];

                    for (tab, icon, label) in tabs.into_iter().rev() {
                        let is_selected = self.active_right_tab == Some(tab);
                        let bg_color = if is_selected { egui::Color32::from_gray(50) } else { egui::Color32::TRANSPARENT };
                        if ui.add(egui::Button::new(egui::RichText::new(icon).size(16.0)).fill(bg_color)).on_hover_text(label).clicked() {
                            if self.active_right_tab == Some(tab) {
                                self.active_right_tab = None;
                            } else {
                                self.active_right_tab = Some(tab);
                            }
                        }
                    }

                    ui.separator();
                    ui.toggle_value(&mut self.is_streaming, "📡 BROADCAST");
                });
            });
        });

        // 4. Central Panel (Execution Plane)
        egui::CentralPanel::default().show(ctx, |ui| {
             match self.active_view {
                 View::Console => views::dj_studio::render(self, ui, &telemetry),
                 View::Player => views::player::render(self, ui, &telemetry),
                 View::Sampler => views::sampler::render(self, ui, &telemetry),
                 View::Mixer => views::mixer::render(self, ui, &telemetry),
                 View::Library => views::library::render(self, ui),
                 View::Topology => views::topology::render(self, ui, &telemetry),
                 View::Modulation => views::modulation::render(self, ui, &telemetry),
                 View::Composer => views::composer::render(self, ui, &telemetry),
                 View::Editor => views::editor::render(self, ui),
                 View::Account => views::account::render(self, ui),
                 View::Breeder => {
                    let mut view = std::mem::replace(&mut self.breeding_view, views::breeder::BreederView::new());
                    views::breeder::BreederView::show(ui, &mut view, &telemetry, self);
                    self.breeding_view = view;
                 }
                 View::Mastering => views::mastering::render(self, ui, &telemetry),
                 View::Broadcast => views::broadcast::render(self, ui),
                 View::Settings => views::settings::render(self, ui),
                 _ => { ui.label("View coming soon..."); }
             }
        });
    }
}

fn main() -> eframe::Result<()> {
    let mut native_options = eframe::NativeOptions::default();
    native_options.renderer = eframe::Renderer::Wgpu;

    eframe::run_native(
        "nullherz Studio",
        native_options,
        Box::new(|cc| {
            let graph = GraphJson { nodes: vec![], edges: vec![], node_assignments: nullherz_traits::NodeAssignmentArray::default() };
            let mut app = InspectorApp::new(graph, cc);

            if let Some(render_state) = &cc.wgpu_render_state {
                // eframe already manages WGPU.
                // We'll mark the renderer as active to enable the GPU-accelerated UI paths.
                app.wgpu_renderer = Some(Arc::new(Mutex::new(nullherz_ui_hal::render::wgpu_backend::WgpuRenderer {
                    device: render_state.device.clone(),
                    queue: render_state.queue.clone(),
                    surface: None,
                    config: None,
                })));

                let wf_renderer = nullherz_ui_hal::render::waveform_renderer::WaveformRenderer::new(
                    &render_state.device,
                    render_state.target_format,
                    1024
                );
                app.waveform_renderer = Some(Arc::new(Mutex::new(wf_renderer)));
            }

            Box::new(app)
        }),
    )
}
