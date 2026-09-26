//! Per-domain UI state, extracted from the former InspectorApp god-struct.
//! Each view's working state lives in its own struct with its own defaults;
//! InspectorApp composes them plus cross-cutting plumbing (command bus,
//! telemetry, theme, renderers).

use crate::{SettingsTab, View};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChannelInputSource {
    Track,
    AudiocardInput,
    Instrument,
}

impl ChannelInputSource {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Track => "Track",
            Self::AudiocardInput => "Audiocard Input",
            Self::Instrument => "Instrument",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Track, Self::AudiocardInput, Self::Instrument]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MasterOutput {
    MainSpeakers,
    Headphones,
    SystemDefault,
    Broadcast,
}

impl MasterOutput {
    pub fn name(&self) -> &'static str {
        match self {
            Self::MainSpeakers => "Main Speakers / Line 1-2",
            Self::Headphones => "Headphones / Line 3-4",
            Self::SystemDefault => "System Default Output",
            Self::Broadcast => "Broadcast Stream Bus",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::MainSpeakers,
            Self::Headphones,
            Self::SystemDefault,
            Self::Broadcast,
        ]
    }
}

/// Dynamic multi-channel console (1..16 channels, default 4): faders, EQ, personality morphs, mastering chain, macros.
#[allow(dead_code)]
pub struct MixerState {
    pub num_channels: usize,
    pub channel_input_sources: [ChannelInputSource; 16],
    pub master_output_source: MasterOutput,
    pub channel_faders: [f32; 16],
    /// Pitch-fader position per deck, as a RATE multiplier. 1.0 is the track's
    /// recorded speed.
    ///
    /// A rate, not a percentage, because that is what the sampler consumes and
    /// what `-12*log2(rate)` needs. Percent is a display unit.
    pub channel_pitch: [f32; 16],
    /// Fader travel per deck, in percent either side of centre: 8, 16 or 50.
    ///
    /// 8 is the default because it is the Technics range and the one beat-matching
    /// muscle memory is built on — the whole fader spans about a semitone and a
    /// half, so a hand movement maps to a tempo change you can actually place.
    /// 50 exists for creative work and is unusable for beat matching.
    pub pitch_range_pct: [f32; 16],
    pub channel_eq_high: [f32; 16],
    pub channel_eq_mid: [f32; 16],
    pub channel_eq_low: [f32; 16],
    pub channel_filter: [f32; 16],
    /// Stereo position, 0.0 = hard left, 0.5 = centre, 1.0 = hard right.
    /// Drives DeckParamType::Pan, which the mixer orchestrator already routes
    /// to the deck's stereo-util node — it simply had no control bound to it.
    pub channel_balance: [f32; 16],
    /// Whether DNA shaping is engaged on each deck.
    ///
    /// False by default: loading a track used to switch the deck's spectral
    /// resynthesis on by itself, costing roughly 10 dB of RMS on audio nobody
    /// asked to have processed.
    pub channel_dna_enabled: [bool; 16],
    /// Stereo width, 1.0 = unmodified. Drives DeckParamType::Width, likewise
    /// already routed and previously unexposed.
    pub channel_width: [f32; 16],
    pub channel_personality_metallic: [f32; 16],
    pub channel_personality_organic: [f32; 16],
    pub channel_personality_warm: [f32; 16],
    pub channel_personality_aggressive: [f32; 16],
    /// Deck tempo-sync (the sampler's quantize/BPM-lock). Engine default is
    /// ON, so the UI must boot showing ON.
    pub channel_sync: [bool; 16],
    /// Slip mode per deck — its own state. (The player view used to reuse
    /// channel_sync for slip, so toggling slip flipped the console's S badge.)
    pub channel_slip: [bool; 16],
    pub quantize_enabled: bool,
    pub master_gain: f32,
    pub crossfader_pos: f32,
    pub channel_peak_hold: [f32; 16],
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
            num_channels: 4,
            channel_input_sources: [ChannelInputSource::Track; 16],
            master_output_source: MasterOutput::MainSpeakers,
            channel_faders: [1.0; 16],
            channel_pitch: [1.0; 16],
            pitch_range_pct: [8.0; 16],
            channel_eq_high: [1.0; 16],
            channel_eq_mid: [1.0; 16],
            channel_eq_low: [1.0; 16],
            channel_filter: [0.5; 16],
            channel_balance: [0.5; 16],
            channel_dna_enabled: [false; 16],
            channel_width: [1.0; 16],
            channel_personality_metallic: [0.0; 16],
            channel_personality_organic: [0.0; 16],
            channel_personality_warm: [0.0; 16],
            channel_personality_aggressive: [0.0; 16],
            channel_sync: [true; 16],
            channel_slip: [false; 16],
            quantize_enabled: true,
            master_gain: 1.0,
            crossfader_pos: 0.5,
            channel_peak_hold: [0.0; 16],
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
    pub now_playing: [Option<u64>; 16],
    /// Per-deck track cache: the console header and waveform used to hit
    /// redb PER FRAME per deck (hundreds of reads/sec at repaint cadence).
    /// Refreshed when the loaded id changes or the library reloads.
    pub cached_tracks: [Option<nullherz_dna::LibraryTrack>; 16],
    pub global_bpm: f32,
    pub focused_deck: usize,
    pub deck_playing: [bool; 16],
    pub global_playing: bool,
    /// Per-deck SYNC latch, mirroring `MixerManager::sync_decks`. **False is
    /// RAW and is the default** — the deck plays at the file's native tempo
    /// until the operator asks otherwise.
    #[allow(dead_code)]
    pub deck_sync: [bool; 16],
    /// Per-deck KEY latch, mirroring `MixerManager::key_sync_decks`. False is
    /// RAW: no pitch shift.
    #[allow(dead_code)]
    pub deck_key_sync: [bool; 16],
    /// Per-deck KEY LOCK (master tempo) latch, mirroring
    /// `MixerManager::key_lock_decks`. False is RAW: tempo changes move pitch,
    /// turntable-style.
    #[allow(dead_code)]
    pub deck_key_lock: [bool; 16],
    /// Active sidecar insert FX attached to each deck channel.
    pub deck_inserts: [Option<String>; 16],
}

impl Default for DeckState {
    fn default() -> Self {
        Self {
            master_deck: Some(0),
            // No tracks are loaded at boot. (The old [Some(1), Some(2), ..]
            // pointed at the retired fixed demo ids — library ids are path
            // hashes now, so the console claimed tracks that don't exist.)
            now_playing: [None; 16],
            cached_tracks: std::array::from_fn(|_| None),
            global_bpm: 128.0,
            focused_deck: 0,
            deck_playing: [false; 16],
            global_playing: false,
            deck_sync: [false; 16],
            deck_key_sync: [false; 16],
            deck_key_lock: [false; 16],
            deck_inserts: std::array::from_fn(|_| None),
        }
    }
}

pub struct LibraryRefreshPayload {
    pub crate_tracks: Vec<nullherz_dna::LibraryTrack>,
    pub all_tracks: Vec<nullherz_dna::LibraryTrack>,
    pub crates: Vec<String>,
    pub smart_crates: Vec<nullherz_dna::SmartCrateDefinition>,
}

/// Library browsing, smart crates, and background loading.
pub struct LibraryState {
    pub active_crate: Option<String>,
    pub search_query: String,
    pub sort: nullherz_dna::TrackSort,
    pub cached_library: Vec<nullherz_dna::LibraryTrack>,
    pub cached_library_raw: Vec<nullherz_dna::LibraryTrack>,
    pub cached_crates: Vec<String>,
    pub cached_smart_crates: Vec<nullherz_dna::SmartCrateDefinition>,
    pub cached_inspected_track: Option<nullherz_dna::LibraryTrack>,
    pub bg_library_loader: Option<std::sync::mpsc::Receiver<LibraryRefreshPayload>>,
    pub library_needs_refresh: bool,
    pub smart_crate_builder_open: bool,
    pub smart_crate_def: nullherz_dna::SmartCrateDefinition,
    pub selected_library_track: Option<u64>,
    /// Track whose details are expanded inline in the list.
    ///
    /// Accordion, not multi-open: one row at a time keeps the list scannable
    /// and keeps the virtualisation maths to a single variable-height row.
    pub expanded_track: Option<u64>,
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
            cached_crates: vec![],
            cached_smart_crates: vec![],
            cached_inspected_track: None,
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
            expanded_track: None,
            playlist_queue: std::collections::VecDeque::new(),
            ingestion_path: "tracks/".to_string(),
            _playlists: vec![],
            last_refresh_time: 0.0,
        }
    }
}

/// Step-sequencer / song-builder grid state.
pub struct ComposerState {
    /// Sample assigned to each sequencer track, independent of the decks.
    pub track_sources: [Option<u64>; 16],
    /// Step grids PER DECK: the composer edits the focused deck's
    /// sequencer, so each deck needs its own grid.
    pub sequencer_grid: [[Vec<f32>; 16]; 4],
    pub selected_composer_track: Option<usize>,
    pub sequencer_active_step: usize,
    pub track_mutes: [bool; 16],
    #[allow(dead_code)]
    pub track_solos: [bool; 16],
    #[allow(dead_code)]
    pub track_volumes: [f32; 16],
    #[allow(dead_code)]
    pub track_pans: [f32; 16],
    #[allow(dead_code)]
    pub track_filters: [f32; 16],
    pub track_targets: [String; 16],
    #[allow(dead_code)]
    pub channel_kinds: [ChannelKind; 16],
    pub record_automation: bool,
    pub _automation_data: std::collections::HashMap<u64, Vec<(f64, f32)>>,
    #[allow(dead_code)]
    pub evolution_strengths: [f32; 16],
    pub auto_pollinate_enabled: bool,
}

impl Default for ComposerState {
    fn default() -> Self {
        Self {
            track_sources: [None; 16],
            sequencer_grid: std::array::from_fn(|_| std::array::from_fn(|_| vec![0.0; 64])),
            selected_composer_track: None,
            sequencer_active_step: 0,
            track_mutes: [false; 16],
            track_solos: [false; 16],
            track_volumes: [1.0; 16],
            track_pans: [0.0; 16],
            track_filters: [0.5; 16],
            track_targets: std::array::from_fn(|_| "(default)".to_string()),
            channel_kinds: std::array::from_fn(|i| if i < 4 { ChannelKind::StereoInput } else { ChannelKind::InstrumentSampler }),
            record_automation: false,
            _automation_data: std::collections::HashMap::new(),
            evolution_strengths: [0.0; 16],
            auto_pollinate_enabled: false,
        }
    }
}

/// Sampler capture/monitoring state.
pub struct SamplerState {
    /// The sample this tool is working on, chosen independently of the decks.
    ///
    /// The sampler used to read `decks.now_playing[decks.focused_deck]`, which
    /// made it a view onto whichever deck happened to have focus rather than a
    /// tool in its own right: you could not chop a sample without first loading
    /// it to a deck, and clicking another deck silently changed what you were
    /// editing. `None` falls back to the focused deck so the old behaviour is
    /// still the default when nothing has been picked.
    pub source_track: Option<u64>,
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
            source_track: None,
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
    pub qwerty_midi_enabled: bool,
    pub qwerty_octave: i8,
    pub qwerty_held_keys: std::collections::HashSet<eframe::egui::Key>,
    pub recent_midi_events: std::collections::VecDeque<nullherz_traits::MidiEvent>,
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
            qwerty_midi_enabled: true,
            qwerty_octave: 0,
            qwerty_held_keys: std::collections::HashSet::new(),
            recent_midi_events: std::collections::VecDeque::with_capacity(30),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
#[allow(dead_code)]
pub enum ChannelKind {
    StereoInput,
    InstrumentSampler,
    InstrumentSynth,
}

#[derive(Clone, PartialEq, Debug)]
pub enum VisualGenerator {
    BioluminescentFluidFlow,
    HarmonicArrangementLattice,
    AbstractQuantumSwarm,
    NeuralFloralMycelium,
    FftSpectrumMesh,
}

impl VisualGenerator {
    pub fn name(&self) -> &'static str {
        match self {
            Self::BioluminescentFluidFlow => "Bioluminescent Fluid Flow",
            Self::HarmonicArrangementLattice => "Harmonic Arrangement Lattice",
            Self::AbstractQuantumSwarm => "Abstract Quantum Swarm",
            Self::NeuralFloralMycelium => "Neural Floral Mycelium",
            Self::FftSpectrumMesh => "3D FFT Spectrum Mesh",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::BioluminescentFluidFlow,
            Self::HarmonicArrangementLattice,
            Self::AbstractQuantumSwarm,
            Self::NeuralFloralMycelium,
            Self::FftSpectrumMesh,
        ]
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum VisualInputSource {
    DeckA,
    DeckB,
    DeckC,
    DeckD,
    MicInput,
    MasterMix,
    MidiTriggerBus,
}

impl VisualInputSource {
    pub fn name(&self) -> &'static str {
        match self {
            Self::DeckA => "Deck A Channel",
            Self::DeckB => "Deck B Channel",
            Self::DeckC => "Deck C Channel",
            Self::DeckD => "Deck D Channel",
            Self::MicInput => "Mic Input",
            Self::MasterMix => "Master Mix Output",
            Self::MidiTriggerBus => "MIDI Trigger Bus",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::MasterMix,
            Self::DeckA,
            Self::DeckB,
            Self::DeckC,
            Self::DeckD,
            Self::MicInput,
            Self::MidiTriggerBus,
        ]
    }
}

/// Standardized rack item for visual channel strips
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct VisualRackItem {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub mix: f32,
}

/// A visual mixer channel strip with multi-input attachments, MIDI control,
/// insert rack, stereo reactivity, and standardized parametric controls.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct VisualChannel {
    pub name: String,
    pub generator: VisualGenerator,
    /// Attached input sources (allows multiple channels attached to one visual)
    pub attached_inputs: Vec<VisualInputSource>,
    pub output_target: String,
    pub gain_sensitivity: f32,
    pub reactivity_smoothing: f32,
    pub stereo_width: f32,
    pub midi_channel: u8,
    pub midi_cc_param: u8,
    pub midi_note_trigger: u8,
    pub midi_learn_active: bool,
    pub inserts: Vec<VisualRackItem>,
    // Standardized parametric controls
    pub param_speed: f32,
    pub param_neural_temp: f32,
    pub param_feedback: f32,
    pub param_color_shift: f32,
    pub param_particle_density: f32,
    pub param_mesh_resolution: f32,
    pub is_muted: bool,
    pub is_solo: bool,
}

impl VisualChannel {
    pub fn new(name: &str, generator: VisualGenerator, inputs: Vec<VisualInputSource>) -> Self {
        Self {
            name: name.to_string(),
            generator,
            attached_inputs: inputs,
            output_target: "Detached Window / Main Viewport".to_string(),
            gain_sensitivity: 1.0,
            reactivity_smoothing: 0.8,
            stereo_width: 1.0,
            midi_channel: 1,
            midi_cc_param: 16,
            midi_note_trigger: 60,
            midi_learn_active: false,
            inserts: vec![
                VisualRackItem {
                    id: "neural-saturator-v".to_string(),
                    name: "Neural Color Saturator".to_string(),
                    enabled: true,
                    mix: 0.8,
                },
                VisualRackItem {
                    id: "bloom-filter".to_string(),
                    name: "Anamorphic Bloom Filter".to_string(),
                    enabled: true,
                    mix: 0.5,
                },
            ],
            param_speed: 1.0,
            param_neural_temp: 0.7,
            param_feedback: 0.3,
            param_color_shift: 0.5,
            param_particle_density: 0.8,
            param_mesh_resolution: 0.6,
            is_muted: false,
            is_solo: false,
        }
    }
}

/// Damped visualizer buffers and Visual Mixer channels.
pub struct VizState {
    pub visualizer_damping: f32,
    pub damped_spectrum: [f32; 128],
    pub damped_goniometer: [f32; 128],
    pub damped_latent: [f32; 16],
    pub damped_peaks: [f32; 16],
    pub damped_master_peaks: [f32; 2],
    /// Previous telemetry deck positions — playing state is DERIVED
    /// (position advanced => playing) instead of kept as a local bool that
    /// drifts from engine truth.
    pub last_deck_positions: [u64; 16],
    /// Consecutive NEW telemetry snapshots in which a deck's position did
    /// not advance. The playing flag only drops after a few still snapshots.
    pub deck_still_snapshots: [u8; 16],
    /// sample_counter of the last telemetry snapshot processed for deck
    /// play-state. The UI repaints faster than telemetry refreshes; deriving
    /// per UI FRAME re-compared the SAME snapshot against itself, flapping
    /// deck_playing to false mid-playback — which turned the player view's
    /// play/stop TOGGLE into a coin flip (a click in a false frame sent
    /// PlayDeck instead of StopDeck: "stop doesn't stop").
    pub last_playstate_counter: u64,

    /// Visual Mixer Channel Strips
    pub channels: Vec<VisualChannel>,
    pub selected_channel_idx: usize,
    /// Specific channel detached into a dedicated visual surface window
    pub detached_channel: Option<usize>,
    #[allow(dead_code)]
    pub master_visual_gain: f32,
    #[allow(dead_code)]
    pub master_visual_brightness: f32,
}

impl Default for VizState {
    fn default() -> Self {
        Self {
            visualizer_damping: 0.1,
            damped_spectrum: [0.0; 128],
            damped_goniometer: [0.0; 128],
            damped_latent: [0.0; 16],
            last_deck_positions: [0; 16],
            deck_still_snapshots: [0; 16],
            last_playstate_counter: 0,
            damped_peaks: [0.0; 16],
            damped_master_peaks: [0.0; 2],
            channels: vec![
                VisualChannel::new(
                    "VIZ 1 — FLUID FLOW",
                    VisualGenerator::BioluminescentFluidFlow,
                    vec![VisualInputSource::MasterMix, VisualInputSource::MidiTriggerBus],
                ),
                VisualChannel::new(
                    "VIZ 2 — HARMONIC LATTICE",
                    VisualGenerator::HarmonicArrangementLattice,
                    vec![VisualInputSource::DeckA, VisualInputSource::DeckB],
                ),
                VisualChannel::new(
                    "VIZ 3 — QUANTUM SWARM",
                    VisualGenerator::AbstractQuantumSwarm,
                    vec![VisualInputSource::DeckC, VisualInputSource::DeckD],
                ),
                VisualChannel::new(
                    "VIZ 4 — MYCELIUM",
                    VisualGenerator::NeuralFloralMycelium,
                    vec![VisualInputSource::MicInput, VisualInputSource::MasterMix],
                ),
            ],
            selected_channel_idx: 0,
            detached_channel: None,
            master_visual_gain: 1.0,
            master_visual_brightness: 1.0,
        }
    }
}

/// Sidecar store browsing and tag filtering state.
pub struct StoreState {
    pub active_tag_filter: Option<String>,
    pub search_query: String,
    #[allow(dead_code)]
    pub selected_sidecar: Option<String>,
    pub store_catalog: sidecar_sdk::SidecarStore,
}

impl Default for StoreState {
    fn default() -> Self {
        Self {
            active_tag_filter: None,
            search_query: String::new(),
            selected_sidecar: None,
            store_catalog: sidecar_sdk::SidecarStore::with_defaults(),
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
    last_positions: &mut [u64; 16],
    still_snapshots: &mut [u8; 16],
    deck_playing: &mut [bool; 16],
) {
    let count = positions.len().min(last_positions.len()).min(still_snapshots.len()).min(deck_playing.len());
    for i in 0..count {
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
        let mut last = [0u64; 16];
        let mut still = [0u8; 16];
        let mut playing = [false; 16];
        for step in 1..=10u64 {
            update_deck_playing(&[step * 256, 0, 0, 0], &mut last, &mut still, &mut playing);
            assert!(playing[0], "moving deck must read as playing at step {}", step);
        }
    }

    #[test]
    fn stopped_deck_needs_consecutive_still_snapshots() {
        let mut last = [0u64; 16];
        let mut still = [0u8; 16];
        let mut playing = [false; 16];
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
        let mut last = [0u64; 16];
        let mut still = [0u8; 16];
        let mut playing = [false; 16];
        update_deck_playing(&[500, 0, 0, 0], &mut last, &mut still, &mut playing);
        update_deck_playing(&[500, 0, 0, 0], &mut last, &mut still, &mut playing);
        update_deck_playing(&[756, 0, 0, 0], &mut last, &mut still, &mut playing);
        assert!(playing[0], "movement after a brief stall must read playing again");
        assert_eq!(still[0], 0, "stall counter must reset on movement");
    }
}
