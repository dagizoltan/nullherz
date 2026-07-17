use serde::{Deserialize, Serialize};
use eframe::egui;
use std::sync::Arc;
use parking_lot::Mutex;
use audio_core::Telemetry;
use nullherz_traits::Command;
use std::sync::mpsc;
use nullherz_dna::GeneticLibrary;

mod views;

pub fn default_coordinate() -> f32 {
    -1.0
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeJson {
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
    pub name: String,
    #[serde(default = "default_coordinate")]
    pub x: f32,
    #[serde(default = "default_coordinate")]
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
    Preferences,
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
    pub(crate) library_db: SharedLibraryDb,
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
    pub(crate) sequencer_grid: [Vec<f32>; 16],
    pub(crate) selected_composer_track: Option<usize>,
    pub(crate) sequencer_active_step: usize,
    pub(crate) sampler_slicer_mode: bool,
    pub(crate) _playlists: Vec<Playlist>,
    pub(crate) cached_library: Vec<nullherz_dna::LibraryTrack>,
    pub(crate) cached_library_raw: Vec<nullherz_dna::LibraryTrack>,
    pub(crate) bg_library_loader: Option<std::sync::mpsc::Receiver<Vec<nullherz_dna::LibraryTrack>>>,
    pub(crate) library_needs_refresh: bool,
    pub(crate) breeding_view: views::breeder::BreederView,
    pub(crate) wgpu_renderer: Option<Arc<Mutex<nullherz_ui_hal::render::wgpu_backend::WgpuRenderer>>>,
    pub(crate) waveform_renderer: Option<Arc<Mutex<nullherz_ui_hal::render::waveform_renderer::WaveformRenderer>>>,
    pub(crate) deck_waveform_renderers: [Option<Arc<Mutex<nullherz_ui_hal::render::waveform_renderer::WaveformRenderer>>>; 4],
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
    pub(crate) track_volumes: [f32; 16],
    pub(crate) track_targets: [String; 16],
    pub(crate) deck_playing: [bool; 4],
    pub(crate) record_automation: bool,
    pub(crate) _automation_data: std::collections::HashMap<u64, Vec<(f64, f32)>>, // target_id -> [(beat, value)]
    pub(crate) sampler_waveform_zoom: f32,
    pub(crate) editor_time_stretch_ratio: f32,
    pub(crate) active_settings_tab: SettingsTab,
    pub(crate) active_backend: nullherz_traits::AudioBackendType,
    pub(crate) active_midi_profile: String,
    pub(crate) config_saved_time: Option<f64>,
    pub(crate) selected_hotload_node_idx: usize,
    // --- Broadcast Settings State ---
    pub(crate) broadcast_url: String,
    pub(crate) broadcast_key: String,
    pub(crate) broadcast_reveal_key: bool,
    pub(crate) broadcast_codec: usize, // 0: Opus, 1: AAC, 2: FLAC
    pub(crate) broadcast_bitrate: f32, // 64 to 512 kbps
    pub(crate) broadcast_state: usize, // 0: Offline, 1: Connecting, 2: Live, 3: Error
    pub(crate) broadcast_error_msg: String,
    pub(crate) broadcast_start_time: Option<f64>,
    pub(crate) p2p_sync_success_toast: Option<f64>,
    pub(crate) export_passport_success_toast: Option<f64>,
    pub(crate) export_passport_error_toast: Option<(f64, String)>,
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
    pub(crate) restore_last_session: bool,
    pub(crate) default_view_on_launch: View,
    pub(crate) autosave_enabled: bool,
    pub(crate) autosave_interval_mins: u32,
    pub(crate) last_saved_time: f64,
    pub(crate) autosave_triggered: Option<f64>,
    pub(crate) shortcuts_enabled: bool,
    pub(crate) global_playing: bool,
    pub(crate) auto_pollinate_enabled: bool,
    pub(crate) _conductor_thread: Option<std::thread::JoinHandle<()>>,
}

impl InspectorApp {
    pub fn trigger_library_refresh(&mut self) {
        self.library_needs_refresh = true;
        let db = self.library_db.clone();
        let crate_name = self.active_crate.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.bg_library_loader = Some(rx);

        std::thread::spawn(move || {
            let tracks = if let Some(ref name) = crate_name {
                db.get_tracks_in_crate(name).unwrap_or_default()
            } else {
                db.list_tracks().unwrap_or_default()
            };
            let _ = tx.send(tracks);
        });
    }

    pub fn get_node_id(&self, name: &str) -> u32 {
        *self.node_map.get(name).unwrap_or(&0)
    }

    pub(crate) fn node_names(&self) -> Vec<(String, u32)> {
        // NOTE: We don't try to filter this down to "instrument-only" nodes yet — there's no
        // processor-type metadata exposed to the UI to do that reliably right now.
        // Routing to a non-instrument node just won't produce sound; it won't crash anything.
        self.node_map.iter().map(|(k, v)| (k.clone(), *v)).collect()
    }

    pub fn new(graph: GraphJson, cc: &eframe::CreationContext<'_>) -> Self {
        let theme = nullherz_ui_hal::Theme::default();
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = theme.bg_canvas;
        visuals.window_fill = theme.bg_surface;
        visuals.extreme_bg_color = theme.bg_inset;
        visuals.override_text_color = Some(theme.text_primary);
        visuals.widgets.noninteractive.bg_fill = theme.bg_surface;
        visuals.widgets.noninteractive.rounding = egui::Rounding::same(theme.radius_md);
        visuals.widgets.inactive.bg_fill = theme.bg_inset;
        visuals.widgets.inactive.rounding = egui::Rounding::same(theme.radius_sm);
        visuals.widgets.hovered.rounding = egui::Rounding::same(theme.radius_sm);
        visuals.widgets.active.rounding = egui::Rounding::same(theme.radius_sm);
        visuals.widgets.open.rounding = egui::Rounding::same(theme.radius_sm);
        visuals.window_rounding = egui::Rounding::same(theme.radius_lg);
        cc.egui_ctx.set_visuals(visuals);

        let mut fonts = egui::FontDefinitions::default();

        // Load Inter-Regular
        let inter_reg_bytes = include_bytes!("../assets/fonts/Inter-Regular.ttf");
        fonts.font_data.insert(
            "Inter-Regular".to_owned(),
            egui::FontData::from_static(inter_reg_bytes),
        );

        // Load Inter-Medium
        let inter_med_bytes = include_bytes!("../assets/fonts/Inter-Medium.ttf");
        fonts.font_data.insert(
            "Inter-Medium".to_owned(),
            egui::FontData::from_static(inter_med_bytes),
        );

        // Insert Inter-Regular at the first position for the Proportional font family
        fonts.families.entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "Inter-Regular".to_owned());

        // Add egui-phosphor icon font
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

        cc.egui_ctx.set_fonts(fonts);

        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let last_telemetry = Arc::new(Mutex::new(None));

        let raw_db = nullherz_dna::LibraryDatabase::load("library.redb").unwrap_or_else(|e| {
            eprintln!("Warning: Failed to load library.redb ({}). Using transient storage.", e);
            nullherz_dna::LibraryDatabase::load(":memory:").expect("Failed to initialize transient LibraryDatabase")
        });
        // Seed or update demo tracks with visual peak metadata
        let mut repair_demo_tracks = false;
        if let Ok(tracks) = raw_db.list_tracks() {
            if tracks.is_empty() {
                repair_demo_tracks = true;
            } else {
                for t in &tracks {
                    if (t.id == 1 || t.id == 2) && t.metadata.peaks.is_empty() {
                        repair_demo_tracks = true;
                    }
                }
            }
        } else {
            repair_demo_tracks = true;
        }

        if repair_demo_tracks {
            println!("Seeding / Repairing demo tracks with visual waveform peaks...");

            // Demo Track A (120 BPM Techno Groove)
            let mut metadata_a = nullherz_traits::SampleMetadata::new_empty();
            metadata_a.bpm = 120.0;
            metadata_a.total_samples = 44100 * 60 * 3; // 3 minutes
            metadata_a.root_key = Some(5.0);

            let mut peaks_a = Vec::with_capacity(2000);
            for i in 0..2000 {
                let beat_phase = (i as f32 * 0.1) % 1.0;
                let kick = (-10.0 * beat_phase).exp();
                let hat = if i % 4 == 2 { (-5.0 * beat_phase).exp() * 0.3 } else { 0.0 };
                let synth = (i as f32 * 0.02).sin().abs() * 0.2;
                peaks_a.push((kick + hat + synth).min(1.0));
            }
            metadata_a.peaks = std::sync::Arc::new(peaks_a);
            let mip_data_a = audio_dsp::util::WaveformProcessor::generate_mip_levels(&metadata_a.peaks, 5);
            metadata_a.mip_waveform.levels = mip_data_a.into_iter().map(std::sync::Arc::new).collect();

            let track_a = nullherz_dna::LibraryTrack {
                id: 1,
                path: "tracks/track_a.wav".to_string(),
                title: "Demo Track A".to_string(),
                artist: "Nullherz".to_string(),
                album: "Demo Album".to_string(),
                genre: "Techno".to_string(),
                energy_level: 0.8,
                metadata: std::sync::Arc::new(metadata_a),
            };
            let _ = raw_db.save_track(&track_a);

            // Demo Track B (124 BPM House Groove)
            let mut metadata_b = nullherz_traits::SampleMetadata::new_empty();
            metadata_b.bpm = 124.0;
            metadata_b.total_samples = 44100 * 60 * 3; // 3 minutes
            metadata_b.root_key = Some(8.0);

            let mut peaks_b = Vec::with_capacity(2000);
            for i in 0..2000 {
                let beat_phase = (i as f32 * 0.08) % 1.0;
                let kick = (-12.0 * beat_phase).exp();
                let snare = if i % 8 == 4 { (-6.0 * beat_phase).exp() * 0.5 } else { 0.0 };
                let bass = (i as f32 * 0.03).cos().abs() * 0.15;
                peaks_b.push((kick + snare + bass).min(1.0));
            }
            metadata_b.peaks = std::sync::Arc::new(peaks_b);
            let mip_data_b = audio_dsp::util::WaveformProcessor::generate_mip_levels(&metadata_b.peaks, 5);
            metadata_b.mip_waveform.levels = mip_data_b.into_iter().map(std::sync::Arc::new).collect();

            let track_b = nullherz_dna::LibraryTrack {
                id: 2,
                path: "tracks/track_b.wav".to_string(),
                title: "Demo Track B".to_string(),
                artist: "Nullherz".to_string(),
                album: "Demo Album".to_string(),
                genre: "House".to_string(),
                energy_level: 0.6,
                metadata: std::sync::Arc::new(metadata_b),
            };
            let _ = raw_db.save_track(&track_b);
        }

        let db_arc = Arc::new(std::sync::Mutex::new(raw_db));
        let library_db_wrapper = SharedLibraryDb(db_arc.clone());

        let (conductor_thread, _conductor) = start_in_process_conductor(cmd_rx, last_telemetry.clone(), db_arc);

        let default_view = View::Console;
        let mut app = Self {
            graph,
            command_sender: cmd_tx,
            last_telemetry,
            _conductor_thread: Some(conductor_thread),
            active_view: default_view,
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
            library_db: library_db_wrapper,
            active_crate: None,
            search_query: String::new(),
            is_streaming: false,
            active_right_tab: Some(RightTab::Library),
            master_deck: Some(0),
            now_playing: [Some(1), Some(2), None, None],
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
            sequencer_grid: std::array::from_fn(|_| vec![0.0; 64]),
            selected_composer_track: None,
            sequencer_active_step: 0,
            sampler_slicer_mode: false,
            _playlists: vec![],
            cached_library: vec![],
            cached_library_raw: vec![],
            bg_library_loader: None,
            library_needs_refresh: true,
            breeding_view: views::breeder::BreederView::new(),
            wgpu_renderer: None,
            waveform_renderer: None,
            deck_waveform_renderers: [None, None, None, None],
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
            focused_deck: 0, // Default to focus Deck A
            track_mutes: [false; 16],
            track_solos: [false; 16],
            track_volumes: [1.0; 16],
            track_targets: std::array::from_fn(|_| "(default)".to_string()),
            deck_playing: [false; 4],
            record_automation: false,
            _automation_data: std::collections::HashMap::new(),
            sampler_waveform_zoom: 1.0,
            editor_time_stretch_ratio: 1.0,
            active_settings_tab: SettingsTab::General,
            active_backend: nullherz_traits::AudioBackendType::Alsa,
            active_midi_profile: "default".to_string(),
            config_saved_time: None,
            selected_hotload_node_idx: 0,
            broadcast_url: "rtmp://gossip.genetic.cloud/live".to_string(),
            broadcast_key: "live_73819283_ab781c981d39281a".to_string(),
            broadcast_reveal_key: false,
            broadcast_codec: 0,
            broadcast_bitrate: 256.0,
            broadcast_state: 0,
            broadcast_error_msg: "Connection timed out (Socket error 111)".to_string(),
            broadcast_start_time: None,
            p2p_sync_success_toast: None,
            export_passport_success_toast: None,
            export_passport_error_toast: None,
            sampler_input_gain: 1.0,
            sampler_monitor_level: 0.0,
            sampler_is_recording: false,
            sampler_is_stereo: true,
            sampler_input_source: 0,
            selected_library_track: None,
            bypassed_nodes: std::collections::HashSet::new(),
            theme: nullherz_ui_hal::Theme::default(),
            last_update_time: 0.0,
            ingestion_path: "tracks/".to_string(),
            playlist_queue: std::collections::VecDeque::new(),
            evolution_strengths: [0.0; 16],
            next_sample_id: 1000,
            editor_selection: None,
            audio_devices: vec!["default".to_string()],
            selected_audio_device: "default".to_string(),
            restore_last_session: false,
            default_view_on_launch: default_view,
            autosave_enabled: false,
            autosave_interval_mins: 5,
            last_saved_time: 0.0,
            autosave_triggered: None,
            shortcuts_enabled: true,
            global_playing: false,
            auto_pollinate_enabled: false,
            node_map: [
                ("deck_a_sampler".to_string(), 0), ("deck_a_gain".to_string(), 4), ("deck_a_filter".to_string(), 3),
                ("deck_b_sampler".to_string(), 4), ("deck_b_gain".to_string(), 8), ("deck_b_filter".to_string(), 7),
                ("deck_c_sampler".to_string(), 8), ("deck_c_gain".to_string(), 12), ("deck_c_filter".to_string(), 11),
                ("deck_d_sampler".to_string(), 12), ("deck_d_gain".to_string(), 16), ("deck_d_filter".to_string(), 15),
                ("master_sum".to_string(), 30), ("master_crossfader".to_string(), 20), ("master_limiter".to_string(), 35),
                ("capture_node".to_string(), 110), ("sequencer_node".to_string(), 70), ("sampler_node".to_string(), 100),
            ].into_iter().collect(),
        };
        app.trigger_library_refresh();
        app
    }

    pub fn deck_color(theme: &nullherz_ui_hal::Theme, i: usize) -> egui::Color32 {
        theme.deck_colors[i % 4]
    }

    fn render_left_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("left_sidebar")
            .resizable(false)
            .default_width(70.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Ω").size(24.0).color(self.theme.accent));
                    ui.add_space(20.0);

                    let top_nav = [
                        (View::Player, egui_phosphor::regular::DISC, "MEDIA PLAYER"),
                        (View::Console, egui_phosphor::regular::RADIO, "DJ CONSOLE"),
                        (View::Composer, egui_phosphor::regular::PIANO_KEYS, "COMPOSER"),
                        (View::Editor, egui_phosphor::regular::SCISSORS, "EDITOR"),
                        (View::Sampler, egui_phosphor::regular::MICROPHONE, "SAMPLER"),
                        (View::Breeder, egui_phosphor::regular::DNA, "DNA BREEDER"),
                        (View::Broadcast, egui_phosphor::regular::BROADCAST, "BROADCAST"),
                    ];

                    let bottom_nav = [
                        (View::Topology, egui_phosphor::regular::SHARE_NETWORK, "TOPOLOGY"),
                        (View::Account, egui_phosphor::regular::USER, "ACCOUNT"),
                        (View::Settings, egui_phosphor::regular::GEAR, "SETTINGS"),
                    ];

                    let mut render_nav_btn = |ui: &mut egui::Ui, view: View, icon: &str, label: &str| {
                        let is_selected = self.active_view == view;
                        let size = egui::vec2(50.0, 50.0);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

                        if response.clicked() {
                            self.active_view = view;
                            ui.ctx().request_repaint();
                        }

                        if is_selected {
                            ui.painter().rect_filled(
                                rect.shrink(1.0),
                                self.theme.radius_md,
                                self.theme.accent.linear_multiply(0.12),
                            );
                            let accent_bar = egui::Rect::from_min_max(
                                rect.left_top() + egui::vec2(2.0, 8.0),
                                rect.left_bottom() + egui::vec2(5.0, -8.0),
                            );
                            ui.painter().rect_filled(accent_bar, 1.5, self.theme.accent);
                        } else if response.hovered() {
                            ui.painter().rect_filled(
                                rect.shrink(1.0),
                                self.theme.radius_md,
                                self.theme.bg_med.linear_multiply(0.4),
                            );
                        }

                        let icon_color = if is_selected {
                            self.theme.accent
                        } else if response.hovered() {
                            self.theme.text_primary
                        } else {
                            self.theme.text_secondary
                        };

                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            icon,
                            egui::FontId::proportional(20.0),
                            icon_color,
                        );

                        response.on_hover_text(label);
                    };

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        for (view, icon, label) in bottom_nav.into_iter().rev() {
                            render_nav_btn(ui, view, icon, label);
                            ui.add_space(10.0);
                        }

                        ui.separator();

                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            egui::ScrollArea::vertical().id_source("nav_scroll").show(ui, |ui| {
                                for (view, icon, label) in top_nav {
                                    render_nav_btn(ui, view, icon, label);
                                    ui.add_space(10.0);
                                }
                            });
                        });
                    });
                });
            });
    }

    fn render_right_sidebar(&mut self, ctx: &egui::Context) {
        if let Some(tab) = self.active_right_tab {
            let right_panel_frame = egui::Frame::none()
                .fill(self.theme.bg_surface)
                .stroke(self.theme.border_stroke)
                .shadow(self.theme.shadow_md);

            egui::SidePanel::right("right_sidebar")
                .resizable(true)
                .min_width(280.0)
                .max_width(600.0)
                .default_width(450.0)
                .frame(right_panel_frame)
                .show(ctx, |ui| {
                    let tab_info = match tab {
                        RightTab::Library => (egui_phosphor::regular::FOLDER_OPEN, "LIBRARY"),
                        RightTab::GeneticCloud => (egui_phosphor::regular::CLOUD, "GENETIC CLOUD"),
                        RightTab::Notifications => (egui_phosphor::regular::BRAIN, "AI & INSIGHTS"),
                        RightTab::Metrics => (egui_phosphor::regular::CHART_BAR, "METRICS"),
                    };

                    egui::Frame::none()
                        .fill(self.theme.bg_surface)
                        .inner_margin(egui::Margin::symmetric(self.theme.space_md, self.theme.space_sm))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{} {}", tab_info.0, tab_info.1))
                                        .strong()
                                        .color(self.theme.accent)
                                        .size(self.theme.type_heading),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(egui_phosphor::regular::X).clicked() {
                                        self.active_right_tab = None;
                                    }
                                });
                            });
                        });

                    ui.separator();
                    ui.add_space(self.theme.space_sm);

                    match tab {
                        RightTab::Library => views::library::render(self, ui),
                        RightTab::GeneticCloud => views::genetic_cloud::render(self, ui),
                        RightTab::Notifications => views::notifications::render(self, ui),
                        RightTab::Metrics => views::metrics::render(self, ui),
                    }
                });
        }
    }

    fn render_bottom_bar(&mut self, ctx: &egui::Context, telemetry: &Option<Telemetry>) {
        egui::TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("nullherz Alpha").size(10.0).color(self.theme.text_disabled));
                ui.separator();

                if let Some(t) = telemetry {
                    ui.label(format!("BPM: {:.1}", t.bpm));
                    ui.separator();
                    ui.label(format!("POS: {:.2}", t.beat_position));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let tabs = [
                        (RightTab::Library, egui_phosphor::regular::FOLDER_OPEN, "LIBRARY"),
                        (RightTab::GeneticCloud, egui_phosphor::regular::CLOUD, "GENETIC CLOUD"),
                        (RightTab::Notifications, egui_phosphor::regular::BRAIN, "AI & INSIGHTS"),
                        (RightTab::Metrics, egui_phosphor::regular::CHART_BAR, "METRICS"),
                    ];

                    for (tab, icon, label) in tabs.into_iter().rev() {
                        let is_selected = self.active_right_tab == Some(tab);
                        let size = egui::vec2(36.0, 36.0);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

                        if response.clicked() {
                            if self.active_right_tab == Some(tab) {
                                self.active_right_tab = None;
                            } else {
                                self.active_right_tab = Some(tab);
                            }
                            ui.ctx().request_repaint();
                        }

                        if is_selected {
                            ui.painter().rect_filled(
                                rect.shrink(1.0),
                                self.theme.radius_sm,
                                self.theme.accent.linear_multiply(0.12),
                            );
                            let accent_bar = egui::Rect::from_min_max(
                                rect.left_bottom() + egui::vec2(6.0, -3.0),
                                rect.right_bottom() + egui::vec2(-6.0, -1.0),
                            );
                            ui.painter().rect_filled(accent_bar, 1.0, self.theme.accent);
                        } else if response.hovered() {
                            ui.painter().rect_filled(
                                rect.shrink(1.0),
                                self.theme.radius_sm,
                                self.theme.bg_med.linear_multiply(0.4),
                            );
                        }

                        let icon_color = if is_selected {
                            self.theme.accent
                        } else if response.hovered() {
                            self.theme.text_primary
                        } else {
                            self.theme.text_secondary
                        };

                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            icon,
                            egui::FontId::proportional(16.0),
                            icon_color,
                        );

                        response.on_hover_text(label);
                    }

                    ui.separator();
                    ui.toggle_value(&mut self.is_streaming, format!("{} BROADCAST", egui_phosphor::regular::BROADCAST));
                });
            });
        });
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let current_time = ctx.input(|i| i.time);
        if self.shortcuts_enabled {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::Space) {
                    if self.global_playing {
                        let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop));
                        self.global_playing = false;
                    } else {
                        let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play));
                        self.global_playing = true;
                    }
                }
                if i.key_pressed(egui::Key::Z) && i.modifiers.command {
                    if i.modifiers.shift {
                        let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Redo));
                    } else {
                        let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Undo));
                    }
                } else if i.key_pressed(egui::Key::Y) && i.modifiers.command {
                    let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Redo));
                }
                if i.key_pressed(egui::Key::S) && i.modifiers.command {
                    let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology));
                    let ports = "Pioneer DDJ-400,Generic MIDI Keyboard".to_string();
                    let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMidiPorts({
                        let mut b = [0u8; 128];
                        let bytes = ports.as_bytes();
                        b[..bytes.len().min(128)].copy_from_slice(&bytes[..bytes.len().min(128)]);
                        b
                    })));
                    self.config_saved_time = Some(current_time);
                    self.autosave_triggered = None;
                }
                if i.key_pressed(egui::Key::Num1) { self.active_view = View::Player; }
                if i.key_pressed(egui::Key::Num2) { self.active_view = View::Console; }
                if i.key_pressed(egui::Key::Num3) { self.active_view = View::Composer; }
                if i.key_pressed(egui::Key::Num4) { self.active_view = View::Editor; }
                if i.key_pressed(egui::Key::Num5) { self.active_view = View::Sampler; }
                if i.key_pressed(egui::Key::Num6) { self.active_view = View::Breeder; }
                if i.key_pressed(egui::Key::Num7) { self.active_view = View::Broadcast; }
                if i.key_pressed(egui::Key::Num8) { self.active_view = View::Topology; }
                if i.key_pressed(egui::Key::Num9) { self.active_view = View::Account; }
            });
        }
    }

    fn handle_autosave(&mut self, current_time: f64) {
        if self.autosave_enabled {
            let interval_secs = (self.autosave_interval_mins as f64) * 60.0;
            if current_time - self.last_saved_time >= interval_secs {
                let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology));
                let ports = "Pioneer DDJ-400,Generic MIDI Keyboard".to_string();
                let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMidiPorts({
                    let mut b = [0u8; 128];
                    let bytes = ports.as_bytes();
                    b[..bytes.len().min(128)].copy_from_slice(&bytes[..bytes.len().min(128)]);
                    b
                })));
                self.last_saved_time = current_time;
                self.config_saved_time = Some(current_time);
                self.autosave_triggered = Some(current_time);
            }
        }
    }
}

impl eframe::App for InspectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let current_time = ctx.input(|i| i.time);

        // --- Background Library Loader Polling ---
        if let Some(ref rx) = self.bg_library_loader {
            if let Ok(tracks) = rx.try_recv() {
                self.cached_library_raw = tracks.clone();
                self.cached_library = tracks;
                self.library_needs_refresh = false;
                self.bg_library_loader = None;
            }
        } else if self.library_needs_refresh {
            self.trigger_library_refresh();
        }

        // Initialize last_saved_time on first loop run if it's 0.0
        if self.last_saved_time == 0.0 {
            self.last_saved_time = current_time;
        }

        // --- Keyboard Shortcuts ---
        self.handle_shortcuts(ctx);

        // --- Autosave Background Job ---
        self.handle_autosave(current_time);

        let is_focused = ctx.input(|i| i.focused);

        // Background Throttling: Skip telemetry processing if unfocused and updated recently (<100ms)
        let should_process = is_focused || (current_time - self.last_update_time) > 0.1;

        let telemetry = if should_process {
            self.last_update_time = current_time;
            *self.last_telemetry.lock()
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
        self.render_left_sidebar(ctx);

        // 2. Right Sidebar (Intelligence Plane - Collapsible)
        self.render_right_sidebar(ctx);

        // 3. Bottom Bar (Status & Global Controls)
        self.render_bottom_bar(ctx, &telemetry);

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
    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

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

                let mut deck_wfs = [None, None, None, None];
                for wf_slot in &mut deck_wfs {
                    let wf = nullherz_ui_hal::render::waveform_renderer::WaveformRenderer::new(
                        &render_state.device,
                        render_state.target_format,
                        1024
                    );
                    *wf_slot = Some(Arc::new(Mutex::new(wf)));
                }
                app.deck_waveform_renderers = deck_wfs;
            }

            Box::new(app)
        }),
    )
}

#[derive(Clone)]
pub struct SharedLibraryDb(pub Arc<std::sync::Mutex<nullherz_dna::LibraryDatabase>>);

impl SharedLibraryDb {
    pub fn list_smart_crates(&self) -> Result<Vec<nullherz_dna::SmartCrateDefinition>, Box<dyn std::error::Error>> {
        self.0.lock().unwrap().list_smart_crates()
    }
    pub fn save_smart_crate(&self, def: &nullherz_dna::SmartCrateDefinition) -> Result<(), Box<dyn std::error::Error>> {
        self.0.lock().unwrap().save_smart_crate(def)
    }
}

impl nullherz_dna::GeneticLibrary for SharedLibraryDb {
    fn get_track(&self, id: u64) -> Result<Option<nullherz_dna::LibraryTrack>, Box<dyn std::error::Error>> {
        self.0.lock().unwrap().get_track(id)
    }
    fn list_tracks(&self) -> Result<Vec<nullherz_dna::LibraryTrack>, Box<dyn std::error::Error>> {
        self.0.lock().unwrap().list_tracks()
    }
    fn save_track(&self, track: &nullherz_dna::LibraryTrack) -> Result<(), Box<dyn std::error::Error>> {
        self.0.lock().unwrap().save_track(track)
    }
    fn add_to_crate(&self, crate_name: &str, track_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.0.lock().unwrap().add_to_crate(crate_name, track_id)
    }
    fn remove_from_crate(&self, crate_name: &str, track_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.0.lock().unwrap().remove_from_crate(crate_name, track_id)
    }
    fn list_crates(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        self.0.lock().unwrap().list_crates()
    }
    fn get_tracks_in_crate(&self, crate_name: &str) -> Result<Vec<nullherz_dna::LibraryTrack>, Box<dyn std::error::Error>> {
        self.0.lock().unwrap().get_tracks_in_crate(crate_name)
    }
    fn query_tracks(&self, genre: Option<&str>, min_bpm: Option<f32>, max_bpm: Option<f32>, root_key: Option<f32>) -> Result<Vec<nullherz_dna::LibraryTrack>, Box<dyn std::error::Error>> {
        self.0.lock().unwrap().query_tracks(genre, min_bpm, max_bpm, root_key)
    }
    fn suggest_matches(&self, target_dna: &nullherz_traits::SoundDNA, limit: usize) -> Result<Vec<(u64, f32)>, Box<dyn std::error::Error>> {
        self.0.lock().unwrap().suggest_matches(target_dna, limit)
    }
    fn remove_track(&self, id: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.0.lock().unwrap().remove_track(id)
    }
}

pub fn start_in_process_conductor(
    cmd_rx: mpsc::Receiver<Command>,
    last_telemetry: Arc<Mutex<Option<Telemetry>>>,
    db_arc: Arc<std::sync::Mutex<nullherz_dna::LibraryDatabase>>,
) -> (std::thread::JoinHandle<()>, Arc<Mutex<nullherz_conductor::Conductor>>) {
    let conductor = nullherz_conductor::Conductor::with_library(db_arc);
    let conductor_arc = Arc::new(Mutex::new(conductor));
    let conductor_clone = conductor_arc.clone();

    let join_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");
        let _guard = rt.enter();

        // Perform setup inside the Tokio context!
        let mut context = {
            let mut cond = conductor_clone.lock();
            let _ = cond.load_system_config();
            let context = cond.setup_engine();

            // Bootstrapping 4-Channel DJ Mixer...
            let mut mixer = nullherz_mixer::MixerManager::new();
            let bootstrap_commands = mixer.create_4channel_mixer();
            cond.apply_mixer_commands(bootstrap_commands);

            if let Some(worker) = cond.analysis_worker.take() {
                worker.start();
            }

            if let Some(monitor) = cond.folder_monitor.take() {
                monitor.start_auto_scan("tracks".to_string());
            }

            cond.sidecar_discovery.start_watcher();

            // Start backend
            let mut backend_type = nullherz_backends::AudioBackendType::Alsa;
            let config_path = "system_config.json";
            if std::path::Path::new(config_path).exists() {
                if let Ok(content) = std::fs::read_to_string(config_path) {
                    if let Ok(config) = serde_json::from_str::<nullherz_conductor::persistence::SystemConfig>(&content) {
                        backend_type = match config.audio_backend.to_lowercase().as_str() {
                            "alsa" => nullherz_backends::AudioBackendType::Alsa,
                            "pipewire" => nullherz_backends::AudioBackendType::Pipewire,
                            "jack" => nullherz_backends::AudioBackendType::Jack,
                            "threaded" => nullherz_backends::AudioBackendType::Threaded,
                            "mock" => nullherz_backends::AudioBackendType::Mock,
                            _ => nullherz_backends::AudioBackendType::Alsa,
                        };
                    }
                }
            }

            // Try starting the preferred backend. If it fails, fallback to Threaded.
            if let Err(e) = cond.start_backend(backend_type) {
                eprintln!(
                    "Failed to start audio backend {:?}: {}. Attempting fallback to Threaded backend...",
                    backend_type, e
                );
                if let Err(fallback_err) = cond.start_backend(nullherz_backends::AudioBackendType::Threaded) {
                    eprintln!("CRITICAL: Failed to start fallback Threaded backend: {}", fallback_err);
                }
            }
            context
        };

        let mut ticker = std::time::Instant::now();
        loop {
            let mut disconnected = false;
            // Scope for locking conductor
            {
                let mut cond = conductor_clone.lock();

                // 1. Process any incoming commands
                loop {
                    match cmd_rx.try_recv() {
                        Ok(cmd) => {
                            cond.apply_mixer_commands(vec![cmd]);
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            break;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }

                if !disconnected {
                    // 2. Tick conductor
                    cond.tick();

                    // 3. Process telemetry
                    while let Some(mut tel) = context.telemetry_consumer.pop() {
                        cond.update_timeline(&mut tel);
                        *last_telemetry.lock() = Some(tel);
                    }
                }
            }

            if disconnected {
                break;
            }

            let elapsed = ticker.elapsed();
            if elapsed < std::time::Duration::from_millis(16) {
                std::thread::sleep(std::time::Duration::from_millis(16) - elapsed);
            }
            ticker = std::time::Instant::now();
        }
    });

    (join_handle, conductor_arc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inspector_command_routing_to_conductor() {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let last_telemetry = Arc::new(Mutex::new(None));

        // Create an in-memory transient LibraryDatabase for testing to avoid lock files
        let raw_db = nullherz_dna::LibraryDatabase::load(":memory:").expect("Failed to initialize transient LibraryDatabase");
        let db_arc = Arc::new(std::sync::Mutex::new(raw_db));

        // Start the in-process conductor thread
        let (_conductor_thread, conductor_arc) = start_in_process_conductor(cmd_rx, last_telemetry, db_arc);

        // Initial state check
        {
            let cond = conductor_arc.lock();
            assert_eq!(cond.active_master_deck, 'A'); // Starts as 'A' by default
        }

        // Send a Command to mutate conductor's state
        cmd_tx.send(Command::Core(nullherz_traits::CoreCommand::SetMasterDeck('C'))).unwrap();

        // Wait for the background thread loop to tick and process the command (approx 100ms for extra safety)
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Verify state mutation on Conductor
        {
            let cond = conductor_arc.lock();
            assert_eq!(cond.active_master_deck, 'C'); // Should have been mutated to 'C'!
        }

        // Send another Command
        cmd_tx.send(Command::Core(nullherz_traits::CoreCommand::SetMasterDeck('D'))).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(100));

        {
            let cond = conductor_arc.lock();
            assert_eq!(cond.active_master_deck, 'D'); // Should have been mutated to 'D'!
        }
    }
}
