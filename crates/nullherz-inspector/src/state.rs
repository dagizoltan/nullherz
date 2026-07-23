//! Per-domain UI state, extracted from the former InspectorApp god-struct.
//! Each view's working state lives in its own struct with its own defaults;
//! InspectorApp composes them plus cross-cutting plumbing (command bus,
//! telemetry, theme, renderers).

use crate::{SettingsTab, View};

/// 4-channel console: faders, EQ, personality morphs, mastering chain, macros.
pub struct MixerState {
    pub channel_faders: [f32; 4],
    pub channel_eq_high: [f32; 4],
    pub channel_eq_mid: [f32; 4],
    pub channel_eq_low: [f32; 4],
    pub channel_filter: [f32; 4],
    pub channel_personality_metallic: [f32; 4],
    pub channel_personality_organic: [f32; 4],
    pub channel_personality_warm: [f32; 4],
    pub channel_personality_aggressive: [f32; 4],
    /// Deck tempo-sync (the sampler's quantize/BPM-lock). Engine default is
    /// ON, so the UI must boot showing ON.
    pub channel_sync: [bool; 4],
    /// Slip mode per deck — its own state. (The player view used to reuse
    /// channel_sync for slip, so toggling slip flipped the console's S badge.)
    pub channel_slip: [bool; 4],
    pub quantize_enabled: bool,
    pub master_gain: f32,
    pub crossfader_pos: f32,
    pub channel_peak_hold: [f32; 4],
    pub master_peak_hold: f32,
    pub _booth_peak_hold: f32,
    pub _rec_peak_hold: f32,
    /// Master tone stage (node "master_eq", params 0/1/2): linear band
    /// gains, 1.0 = flat. Live — the mastering view binds knobs to these.
    pub mastering_eq_low: f32,
    pub mastering_eq_mid: f32,
    pub mastering_eq_high: f32,
    pub macros: [f32; 8],
    pub _macro_names: [String; 8],
    pub personality_macro_mode: bool,
    pub spectral_window_shape: u32,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            channel_faders: [1.0; 4],
            channel_eq_high: [1.0; 4],
            channel_eq_mid: [1.0; 4],
            channel_eq_low: [1.0; 4],
            channel_filter: [0.5; 4],
            channel_personality_metallic: [0.0; 4],
            channel_personality_organic: [0.0; 4],
            channel_personality_warm: [0.0; 4],
            channel_personality_aggressive: [0.0; 4],
            channel_sync: [true; 4],
            channel_slip: [false; 4],
            quantize_enabled: true,
            master_gain: 1.0,
            crossfader_pos: 0.5,
            channel_peak_hold: [0.0; 4],
            master_peak_hold: 0.0,
            _booth_peak_hold: 0.0,
            _rec_peak_hold: 0.0,
            mastering_eq_low: 1.0,
            mastering_eq_mid: 1.0,
            mastering_eq_high: 1.0,
            macros: [0.0; 8],
            _macro_names: std::array::from_fn(|i| format!("MACRO {}", i + 1)),
            personality_macro_mode: false,
            spectral_window_shape: 0,
        }
    }
}

/// Deck transport: what's loaded, what's playing, which deck has focus.
pub struct DeckState {
    pub master_deck: Option<usize>,
    pub now_playing: [Option<u64>; 4],
    /// Per-deck track cache: the console header and waveform used to hit
    /// redb PER FRAME per deck (hundreds of reads/sec at repaint cadence).
    /// Refreshed when the loaded id changes or the library reloads.
    pub cached_tracks: [Option<nullherz_dna::LibraryTrack>; 4],
    pub global_bpm: f32,
    pub focused_deck: usize,
    pub deck_playing: [bool; 4],
    pub global_playing: bool,
}

impl Default for DeckState {
    fn default() -> Self {
        Self {
            master_deck: Some(0),
            // No tracks are loaded at boot. (The old [Some(1), Some(2), ..]
            // pointed at the retired fixed demo ids — library ids are path
            // hashes now, so the console claimed tracks that don't exist.)
            now_playing: [None; 4],
            cached_tracks: std::array::from_fn(|_| None),
            global_bpm: 128.0,
            focused_deck: 0,
            deck_playing: [false; 4],
            global_playing: false,
        }
    }
}

/// Library browsing, smart crates, and background loading.
pub struct LibraryState {
    pub active_crate: Option<String>,
    pub search_query: String,
    pub sort: nullherz_dna::TrackSort,
    pub cached_library: Vec<nullherz_dna::LibraryTrack>,
    pub cached_library_raw: Vec<nullherz_dna::LibraryTrack>,
    pub bg_library_loader: Option<std::sync::mpsc::Receiver<Vec<nullherz_dna::LibraryTrack>>>,
    pub library_needs_refresh: bool,
    pub smart_crate_builder_open: bool,
    pub smart_crate_def: nullherz_dna::SmartCrateDefinition,
    pub selected_library_track: Option<u64>,
    pub playlist_queue: std::collections::VecDeque<u64>,
    pub ingestion_path: String,
    pub _playlists: Vec<crate::Playlist>,
    /// Last background-refresh completion time; drives periodic re-polling so
    /// tracks analyzed AFTER startup appear without user action.
    pub last_refresh_time: f64,
}

impl Default for LibraryState {
    fn default() -> Self {
        Self {
            active_crate: None,
            search_query: String::new(),
            sort: nullherz_dna::TrackSort::default(),
            cached_library: vec![],
            cached_library_raw: vec![],
            bg_library_loader: None,
            library_needs_refresh: true,
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
            selected_library_track: None,
            playlist_queue: std::collections::VecDeque::new(),
            ingestion_path: "tracks/".to_string(),
            _playlists: vec![],
            last_refresh_time: 0.0,
        }
    }
}

/// Step-sequencer / song-builder grid state.
pub struct ComposerState {
    /// Step grids PER DECK: the composer edits the focused deck's
    /// sequencer, so each deck needs its own grid — one shared grid showed
    /// deck A's steps no matter which deck you were editing.
    pub sequencer_grid: [[Vec<f32>; 16]; 4],
    pub selected_composer_track: Option<usize>,
    pub sequencer_active_step: usize,
    pub track_mutes: [bool; 16],
    pub track_solos: [bool; 16],
    pub track_volumes: [f32; 16],
    pub track_targets: [String; 16],
    pub record_automation: bool,
    pub _automation_data: std::collections::HashMap<u64, Vec<(f64, f32)>>,
    pub evolution_strengths: [f32; 16],
    pub auto_pollinate_enabled: bool,
}

impl Default for ComposerState {
    fn default() -> Self {
        Self {
            sequencer_grid: std::array::from_fn(|_| std::array::from_fn(|_| vec![0.0; 64])),
            selected_composer_track: None,
            sequencer_active_step: 0,
            track_mutes: [false; 16],
            track_solos: [false; 16],
            track_volumes: [1.0; 16],
            track_targets: std::array::from_fn(|_| "(default)".to_string()),
            record_automation: false,
            _automation_data: std::collections::HashMap::new(),
            evolution_strengths: [0.0; 16],
            auto_pollinate_enabled: false,
        }
    }
}

/// Sampler capture/monitoring state.
pub struct SamplerState {
    pub sampler_slicer_mode: bool,
    pub sampler_waveform_zoom: f32,
    pub sampler_input_gain: f32,
    pub sampler_monitor_level: f32,
    pub sampler_is_recording: bool,
    pub sampler_is_stereo: bool,
    pub sampler_input_source: usize,
    pub next_sample_id: u64,
}

impl Default for SamplerState {
    fn default() -> Self {
        Self {
            sampler_slicer_mode: false,
            sampler_waveform_zoom: 1.0,
            sampler_input_gain: 1.0,
            sampler_monitor_level: 0.0,
            sampler_is_recording: false,
            sampler_is_stereo: true,
            sampler_input_source: 0,
            next_sample_id: 1000,
        }
    }
}

/// Audio editor selection and stretch controls.
pub struct EditorState {
    pub editor_selection: Option<(f32, f32)>,
    pub editor_time_stretch_ratio: f32,
}

impl Default for EditorState {
    fn default() -> Self {
        Self { editor_selection: None, editor_time_stretch_ratio: 1.0 }
    }
}

/// Streaming/broadcast panel state.
pub struct BroadcastState {
    pub broadcast_url: String,
    pub broadcast_key: String,
    pub broadcast_reveal_key: bool,
    pub broadcast_codec: usize,
    pub broadcast_bitrate: f32,
    pub broadcast_state: usize,
    pub broadcast_error_msg: String,
    pub broadcast_start_time: Option<f64>,
    pub is_streaming: bool,
}

impl Default for BroadcastState {
    fn default() -> Self {
        Self {
            broadcast_url: "rtmp://gossip.genetic.cloud/live".to_string(),
            broadcast_key: "live_73819283_ab781c981d39281a".to_string(),
            broadcast_reveal_key: false,
            broadcast_codec: 0,
            broadcast_bitrate: 256.0,
            broadcast_state: 0,
            broadcast_error_msg: "Connection timed out (Socket error 111)".to_string(),
            broadcast_start_time: None,
            is_streaming: false,
        }
    }
}

/// Settings panel state and persisted preferences.
pub struct SettingsState {
    pub active_settings_tab: SettingsTab,
    pub active_backend: nullherz_traits::AudioBackendType,
    pub active_midi_profile: String,
    pub config_saved_time: Option<f64>,
    pub audio_devices: Vec<String>,
    /// Retained for the future device-selection command.
    pub _selected_audio_device: String,
    pub restore_last_session: bool,
    pub default_view_on_launch: View,
    pub autosave_enabled: bool,
    pub autosave_interval_mins: u32,
    pub last_saved_time: f64,
    pub autosave_triggered: Option<f64>,
    pub shortcuts_enabled: bool,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            active_settings_tab: SettingsTab::General,
            active_backend: nullherz_traits::AudioBackendType::Alsa,
            active_midi_profile: "default".to_string(),
            config_saved_time: None,
            audio_devices: vec!["default".to_string()],
            _selected_audio_device: "default".to_string(),
            restore_last_session: false,
            default_view_on_launch: View::Console,
            autosave_enabled: false,
            autosave_interval_mins: 5,
            last_saved_time: 0.0,
            autosave_triggered: None,
            shortcuts_enabled: true,
        }
    }
}

/// Damped visualizer buffers (spectrum, goniometer, latent space, meters).
pub struct VizState {
    pub visualizer_damping: f32,
    pub damped_spectrum: [f32; 128],
    pub damped_goniometer: [f32; 128],
    pub damped_latent: [f32; 16],
    pub damped_peaks: [f32; 4],
    pub damped_master_peaks: [f32; 2],
    /// Previous telemetry deck positions — playing state is DERIVED
    /// (position advanced => playing) instead of kept as a local bool that
    /// drifts from engine truth.
    pub last_deck_positions: [u64; 4],
    /// Consecutive NEW telemetry snapshots in which a deck's position did
    /// not advance. The playing flag only drops after a few still snapshots.
    pub deck_still_snapshots: [u8; 4],
    /// sample_counter of the last telemetry snapshot processed for deck
    /// play-state. The UI repaints faster than telemetry refreshes; deriving
    /// per UI FRAME re-compared the SAME snapshot against itself, flapping
    /// deck_playing to false mid-playback — which turned the player view's
    /// play/stop TOGGLE into a coin flip (a click in a false frame sent
    /// PlayDeck instead of StopDeck: "stop doesn't stop").
    pub last_playstate_counter: u64,
}

impl Default for VizState {
    fn default() -> Self {
        Self {
            visualizer_damping: 0.1,
            damped_spectrum: [0.0; 128],
            damped_goniometer: [0.0; 128],
            damped_latent: [0.0; 16],
            last_deck_positions: [0; 4],
            deck_still_snapshots: [0; 4],
            last_playstate_counter: 0,
            damped_peaks: [0.0; 4],
            damped_master_peaks: [0.0; 2],
        }
    }
}

/// Topology-editor view state (cable drags, bypasses, node naming).
pub struct TopologyViewState {
    pub active_connection_source: Option<(u32, u32)>,
    pub active_node_drag: Option<u32>,
    pub bypassed_nodes: std::collections::HashSet<u32>,
    pub selected_hotload_node_idx: usize,
    pub node_map: std::collections::HashMap<String, u32>,
}

impl Default for TopologyViewState {
    fn default() -> Self {
        Self {
            active_connection_source: None,
            active_node_drag: None,
            bypassed_nodes: std::collections::HashSet::new(),
            selected_hotload_node_idx: 0,
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
}

/// Derive per-deck playing state from a telemetry snapshot. Must be called
/// once per NEW snapshot (caller gates on `sample_counter` advancing): a deck
/// is playing iff its playhead advanced, and only counts as stopped after
/// `STILL_SNAPSHOTS_TO_STOP` consecutive still snapshots (a slow playback
/// rate can hold a u64 position across a block without being stopped).
pub const STILL_SNAPSHOTS_TO_STOP: u8 = 3;

pub fn update_deck_playing(
    positions: &[u64; 4],
    last_positions: &mut [u64; 4],
    still_snapshots: &mut [u8; 4],
    deck_playing: &mut [bool; 4],
) {
    for i in 0..4 {
        let pos = positions[i];
        if pos != 0 && pos != last_positions[i] {
            still_snapshots[i] = 0;
            deck_playing[i] = true;
        } else {
            still_snapshots[i] = still_snapshots[i].saturating_add(1);
            if still_snapshots[i] >= STILL_SNAPSHOTS_TO_STOP {
                deck_playing[i] = false;
            }
        }
        last_positions[i] = pos;
    }
}

#[cfg(test)]
mod playstate_tests {
    use super::*;

    /// The bug this kills: the same snapshot processed twice (UI frames
    /// outpacing telemetry) must NOT mark a moving deck as stopped. The
    /// caller gates on sample_counter, so this function simply never runs
    /// for a repeat — asserted here by contract: consecutive DIFFERENT
    /// positions always yield playing=true.
    #[test]
    fn advancing_position_is_always_playing() {
        let mut last = [0u64; 4];
        let mut still = [0u8; 4];
        let mut playing = [false; 4];
        for step in 1..=10u64 {
            update_deck_playing(&[step * 256, 0, 0, 0], &mut last, &mut still, &mut playing);
            assert!(playing[0], "moving deck must read as playing at step {}", step);
        }
    }

    #[test]
    fn stopped_deck_needs_consecutive_still_snapshots() {
        let mut last = [0u64; 4];
        let mut still = [0u8; 4];
        let mut playing = [false; 4];
        update_deck_playing(&[1_000, 0, 0, 0], &mut last, &mut still, &mut playing);
        assert!(playing[0]);
        // One or two still snapshots: still playing (slow-rate tolerance).
        update_deck_playing(&[1_000, 0, 0, 0], &mut last, &mut still, &mut playing);
        assert!(playing[0]);
        update_deck_playing(&[1_000, 0, 0, 0], &mut last, &mut still, &mut playing);
        assert!(playing[0]);
        // Third still snapshot: stopped.
        update_deck_playing(&[1_000, 0, 0, 0], &mut last, &mut still, &mut playing);
        assert!(!playing[0], "deck still after {} snapshots must read stopped", STILL_SNAPSHOTS_TO_STOP);
    }

    #[test]
    fn brief_stall_recovers_immediately() {
        let mut last = [0u64; 4];
        let mut still = [0u8; 4];
        let mut playing = [false; 4];
        update_deck_playing(&[500, 0, 0, 0], &mut last, &mut still, &mut playing);
        update_deck_playing(&[500, 0, 0, 0], &mut last, &mut still, &mut playing);
        update_deck_playing(&[756, 0, 0, 0], &mut last, &mut still, &mut playing);
        assert!(playing[0], "movement after a brief stall must read playing again");
        assert_eq!(still[0], 0, "stall counter must reset on movement");
    }
}
