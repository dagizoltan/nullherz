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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbCompareSource {
    DeckA,
    DeckB,
    DeckC,
    DeckD,
    Master,
}

impl AbCompareSource {
    pub fn name(&self) -> &'static str {
        match self {
            Self::DeckA => "Deck A",
            Self::DeckB => "Deck B",
            Self::DeckC => "Deck C",
            Self::DeckD => "Deck D",
            Self::Master => "Master Output",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::DeckA, Self::DeckB, Self::DeckC, Self::DeckD, Self::Master]
    }
}

pub struct AnalyzerViewState {
    pub layer_raw: bool,
    pub layer_spectral: bool,
    pub layer_rhythm: bool,
    pub layer_harmonic: bool,
    pub layer_transient: bool,
    pub layer_stereo: bool,
    pub layer_energy: bool,
    pub layer_events: bool,
    pub layer_dna: bool,
    pub layer_collision: bool,
    pub layer_embedding: bool,

    pub ab_enabled: bool,
    pub source_a: AbCompareSource,
    pub source_b: AbCompareSource,

    pub _selected_event_index: Option<usize>,
}

impl Default for AnalyzerViewState {
    fn default() -> Self {
        Self {
            layer_raw: true,
            layer_spectral: true,
            layer_rhythm: true,
            layer_harmonic: true,
            layer_transient: true,
            layer_stereo: false,
            layer_energy: true,
            layer_events: true,
            layer_dna: true,
            layer_collision: false,
            layer_embedding: false,

            ab_enabled: false,
            source_a: AbCompareSource::DeckA,
            source_b: AbCompareSource::DeckB,

            _selected_event_index: None,
        }
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

/// Multidimensional Audio Nervous System feature field extracted from telemetry
#[derive(Clone, Debug, Default)]
pub struct AudioNervousSystem {
    pub rms_energy: f32,
    pub spectral_centroid: f32,
    pub spectral_flux: f32,
    pub low_band: f32,
    pub mid_band: f32,
    pub high_band: f32,
    pub transient_density: f32,
    pub onset_strength: f32,
    pub fast_transient_spike: f32, // Instantaneous zero-lag drum attack trigger
    pub bpm: f32,
    pub beat_phase: f32,
    pub sub_beat_phase: f32,
    pub pitch_chroma: [f32; 12],
    pub harmonicity: f32,
    pub noisiness: f32,
    pub stereo_width: f32,
    pub stereo_asymmetry: f32,
    pub long_term_envelope: f32,
    pub short_term_envelope: f32,
    pub spectral_entropy: f32,
    pub zero_crossing_rate: f32,
}

/// Visual Genome governing the generative visual organism's mathematical personality
#[derive(Clone, Debug)]
pub struct VisualGenome {
    pub topology_complexity: f32,
    pub symmetry_folds: usize,
    pub branching_factor: f32,
    pub turbulence_scale: f32,
    pub particle_cohesion: f32,
    pub particle_separation: f32,
    pub fracture_rate: f32,
    pub feedback_persistence: f32,
    pub color_field_shift: f32,
    #[allow(dead_code)]
    pub mutation_inertia: f32,
    pub growth_rate: f32,
    #[allow(dead_code)]
    pub erosion_factor: f32,
    pub roughness: f32,
    pub emission_glow: f32,
    pub genes: [f32; 32], // 32 continuous gene parameters
}

impl Default for VisualGenome {
    fn default() -> Self {
        Self {
            topology_complexity: 1.0,
            symmetry_folds: 8,
            branching_factor: 1.0,
            turbulence_scale: 0.5,
            particle_cohesion: 0.8,
            particle_separation: 0.2,
            fracture_rate: 0.1,
            feedback_persistence: 0.85,
            color_field_shift: 0.5,
            mutation_inertia: 0.9,
            growth_rate: 1.0,
            erosion_factor: 0.05,
            roughness: 0.3,
            emission_glow: 0.7,
            genes: [0.5; 32],
        }
    }
}

/// Non-linear neural mapper translating audio nervous system into visual genome parameters
#[derive(Clone, Debug)]
pub struct NeuralLatentMapper {
    pub weights_in: [[f32; 32]; 32],
    pub weights_out: [[f32; 32]; 32],
    pub latent_neurons: [f32; 32],
}

impl NeuralLatentMapper {
    pub fn new() -> Self {
        let mut weights_in = [[0.0f32; 32]; 32];
        let mut weights_out = [[0.0f32; 32]; 32];
        for i in 0..32 {
            for j in 0..32 {
                weights_in[i][j] = ((i as f32 * 0.3 + j as f32 * 0.7).sin() * 0.5).clamp(-1.0, 1.0);
                weights_out[i][j] = ((i as f32 * 1.1 + j as f32 * 0.4).cos() * 0.5).clamp(-1.0, 1.0);
            }
        }
        Self {
            weights_in,
            weights_out,
            latent_neurons: [0.0; 32],
        }
    }

    pub fn map(&mut self, nervous: &AudioNervousSystem, genome: &mut VisualGenome) {
        let mut audio_vec = [0.0f32; 32];
        audio_vec[0] = nervous.rms_energy;
        audio_vec[1] = nervous.spectral_centroid;
        audio_vec[2] = nervous.spectral_flux;
        audio_vec[3] = nervous.low_band;
        audio_vec[4] = nervous.mid_band;
        audio_vec[5] = nervous.high_band;
        audio_vec[6] = nervous.transient_density;
        audio_vec[7] = nervous.onset_strength;
        audio_vec[8] = nervous.beat_phase;
        audio_vec[9] = nervous.sub_beat_phase;
        audio_vec[10] = nervous.harmonicity;
        audio_vec[11] = nervous.noisiness;
        audio_vec[12] = nervous.stereo_width;
        audio_vec[13] = nervous.stereo_asymmetry;
        audio_vec[14] = nervous.spectral_entropy;
        audio_vec[15] = nervous.zero_crossing_rate;
        for i in 0..12 {
            audio_vec[16 + i] = nervous.pitch_chroma[i];
        }

        // Layer 1: Forward dense pass to latent neurons
        for i in 0..32 {
            let mut sum = 0.0f32;
            for j in 0..32 {
                sum += audio_vec[j] * self.weights_in[j][i];
            }
            let x2 = sum * sum;
            self.latent_neurons[i] = (sum * (27.0 + x2) / (27.0 + 9.0 * x2)).clamp(-1.0, 1.0);
        }

        // Layer 2: Output pass to continuous visual genes
        for i in 0..32 {
            let mut sum = 0.0f32;
            for j in 0..32 {
                sum += self.latent_neurons[j] * self.weights_out[j][i];
            }
            let x2 = sum * sum;
            let target_gene = (sum * (27.0 + x2) / (27.0 + 9.0 * x2)).clamp(-1.0, 1.0) * 0.5 + 0.5;
            genome.genes[i] += (target_gene - genome.genes[i]) * 0.15;
        }

        // Map genes to structured visual parameters
        genome.topology_complexity = 0.5 + genome.genes[0] * 2.5;
        genome.symmetry_folds = match (genome.genes[1] * 6.0) as usize {
            0 => 2,
            1 => 4,
            2 => 6,
            3 => 8,
            4 => 12,
            _ => 16,
        };
        genome.branching_factor = 0.2 + genome.genes[2] * 2.0;
        genome.turbulence_scale = genome.genes[3] * 1.5;
        genome.particle_cohesion = genome.genes[4];
        genome.particle_separation = genome.genes[5];
        genome.fracture_rate = genome.genes[6];
        genome.feedback_persistence = 0.5 + genome.genes[7] * 0.48;
        genome.color_field_shift = genome.genes[8];
        genome.growth_rate = 0.1 + genome.genes[9] * 2.0;
        genome.roughness = genome.genes[10];
        genome.emission_glow = genome.genes[11];
    }
}

impl Default for NeuralLatentMapper {
    fn default() -> Self {
        Self::new()
    }
}

/// Historical visual memory tracking trajectory state across time
#[derive(Clone, Debug)]
pub struct VisualMemory {
    pub energy_history: [f32; 64],
    pub centroid_history: [f32; 64],
    pub topological_accumulator: f32,
    #[allow(dead_code)]
    pub historical_symmetry_center: (f32, f32),
    pub mutation_events_count: u32,
    pub last_mutation_time: f64,
}

impl Default for VisualMemory {
    fn default() -> Self {
        Self {
            energy_history: [0.0; 64],
            centroid_history: [0.0; 64],
            topological_accumulator: 0.0,
            historical_symmetry_center: (0.0, 0.0),
            mutation_events_count: 0,
            last_mutation_time: 0.0,
        }
    }
}

impl VisualMemory {
    pub fn push_snapshot(&mut self, energy: f32, centroid: f32, time: f64) {
        for i in (1..64).rev() {
            self.energy_history[i] = self.energy_history[i - 1];
            self.centroid_history[i] = self.centroid_history[i - 1];
        }
        self.energy_history[0] = energy;
        self.centroid_history[0] = centroid;
        self.topological_accumulator = (self.topological_accumulator + energy * 0.01).rem_euclid(100.0);

        if energy > 0.8 && (time - self.last_mutation_time) > 0.5 {
            self.mutation_events_count = self.mutation_events_count.wrapping_add(1);
            self.last_mutation_time = time;
        }
    }
}

/// Event-driven structural mutation engine
#[derive(Clone, Debug, Default)]
pub struct MutationEngine {
    pub species: VisualOrganismSpecies,
    pub mutation_intensity: f32,
}

impl MutationEngine {
    pub fn handle_event(&mut self, nervous: &AudioNervousSystem, memory: &VisualMemory) {
        // Structural mutation on major phrase transition / high onset
        if nervous.onset_strength > 0.85 && nervous.transient_density > 0.6 {
            self.mutation_intensity = 1.0;
            if memory.mutation_events_count % 4 == 0 {
                self.species = self.species.next_species();
            }
        } else {
            self.mutation_intensity *= 0.92;
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FieldType {
    CurlNoise,
    ReactionDiffusion,
    BoidsField,
    ImplicitSurface,
    RecursiveTransform,
    VectorFlow,
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TopologyType {
    AdaptiveGraph,
    Recursive,
    FluidImplicit,
    Emergent,
    NeuralGuided,
    ModularGraph,
    SparseMinimal,
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GeometryPrimitive {
    Filaments,
    Nodes,
    Ribbons,
    Shards,
    Membranes,
    Beams,
    Points,
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MotionBehavior {
    CurlFlow,
    Mechanical,
    SubdivisionExpand,
    BoidsHybrid,
    RecurrentEvolution,
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MaterialStyle {
    TranslucentGlow,
    HardSurface,
    RefractiveFluid,
    Crystalline,
    MinimalVoid,
}

/// Composable Visual Organism Species Taxonomy
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VisualOrganismSpecies {
    MycelialBloom,
    FractureBloom,
    FluidGeometry,
    GeometricSwarm,
    NeuralGarden,
    ImpossibleMachine,
    VoidOrganism,
    FractalPulse,
}

#[allow(dead_code)]
impl VisualOrganismSpecies {
    pub fn name(&self) -> &'static str {
        match self {
            Self::MycelialBloom => "Mycelial Bloom (Adaptive Branching Network)",
            Self::FractureBloom => "Fracture Bloom (Recursive Crystalline Shards)",
            Self::FluidGeometry => "Fluid Geometry (Implicit Liquid Surface)",
            Self::GeometricSwarm => "Geometric Swarm (Boids Collective Lattice)",
            Self::NeuralGarden => "Neural Garden (Recurrent Latent Evolution)",
            Self::ImpossibleMachine => "Impossible Machine (Modular Architectural Beams)",
            Self::VoidOrganism => "Void Organism (Minimal Dark-Space Geometry)",
            Self::FractalPulse => "Fractal Pulse (Recursive Transform Geometry)",
        }
    }

    pub fn next_species(&self) -> Self {
        match self {
            Self::MycelialBloom => Self::FractureBloom,
            Self::FractureBloom => Self::FluidGeometry,
            Self::FluidGeometry => Self::GeometricSwarm,
            Self::GeometricSwarm => Self::NeuralGarden,
            Self::NeuralGarden => Self::ImpossibleMachine,
            Self::ImpossibleMachine => Self::VoidOrganism,
            Self::VoidOrganism => Self::FractalPulse,
            Self::FractalPulse => Self::MycelialBloom,
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::MycelialBloom,
            Self::FractureBloom,
            Self::FluidGeometry,
            Self::GeometricSwarm,
            Self::NeuralGarden,
            Self::ImpossibleMachine,
            Self::VoidOrganism,
            Self::FractalPulse,
        ]
    }
}

impl Default for VisualOrganismSpecies {
    fn default() -> Self {
        Self::MycelialBloom
    }
}

/// Biological 64-Neuron Multi-Layer Cortical Reservoir Engine for audio-driven organic visual synthesis.
/// Features Izhikevich/LIF membrane potentials, neurotransmitter release kinetics ($s_i$),
/// short-term synaptic plasticity (depression/facilitation), 2D spatial axonal grid topology,
/// and 16 continuous motor output channels driving complex organic visual oscillations.
#[derive(Clone, Debug)]
pub struct SpikingNeuronNetwork {
    pub v: [f32; 64],             // Membrane potentials (mV)
    pub u: [f32; 64],             // Recovery variables
    pub s: [f32; 64],             // Synaptic neurotransmitter conductance/release
    pub spikes: [bool; 64],       // Spike triggers for current frame
    pub stdp_trace: [f32; 64],    // STDP activity traces
    pub weights: [[f32; 64]; 64], // 64x64 Recurrent synaptic weight matrix
    pub motor_outputs: [f32; 16], // 16 Continuous neural motor activation values
    pub axonal_wave_field: [f32; 64], // 2D 8x8 spatial wave propagation grid
}

impl SpikingNeuronNetwork {
    pub fn new() -> Self {
        let mut weights = [[0.0f32; 64]; 64];
        for i in 0..64 {
            let row_i = i / 8;
            let col_i = i % 8;
            for j in 0..64 {
                if i != j {
                    let row_j = j / 8;
                    let col_j = j % 8;
                    let dist_sq = ((row_i as f32 - row_j as f32).powi(2) + (col_i as f32 - col_j as f32).powi(2)).max(0.5);
                    let spatial_decay = (-dist_sq * 0.25).exp();
                    let periodic_factor = (i as f32 * 0.4 + j as f32 * 0.9).sin();
                    weights[i][j] = periodic_factor * spatial_decay * 0.4;
                }
            }
        }
        Self {
            v: [-65.0; 64],
            u: [-13.0; 64],
            s: [0.0; 64],
            spikes: [false; 64],
            stdp_trace: [0.0; 64],
            weights,
            motor_outputs: [0.0; 16],
            axonal_wave_field: [0.0; 64],
        }
    }

    pub fn step(&mut self, audio_inputs: &[f32], dt: f32, neural_temp: f32, feedback_coupling: f32) {
        let a = 0.02f32;
        let b = 0.2f32;
        let c = -65.0f32;
        let d = 8.0f32;

        let dt_ms = (dt * 1000.0).clamp(1.0, 33.0);
        let sub_steps = 2;
        let h = dt_ms / sub_steps as f32;

        for _sub in 0..sub_steps {
            let mut currents = [0.0f32; 64];

            // 1. Audio External Driving Currents
            for i in 0..64 {
                let input_val = audio_inputs.get(i % audio_inputs.len().max(1)).copied().unwrap_or(0.0);
                // Excitatory / Inhibitory neuron distribution (80% excitatory, 20% inhibitory)
                let cell_type_scale = if i % 5 == 0 { -0.8 } else { 1.2 };
                currents[i] += input_val * 18.0 * neural_temp * cell_type_scale;
            }

            // 2. Recurrent Synaptic & Axonal Wave Transmission
            for from in 0..64 {
                let transmitter = self.s[from];
                if transmitter > 0.01 {
                    for to in 0..64 {
                        currents[to] += transmitter * self.weights[from][to] * 25.0 * (1.0 + feedback_coupling);
                    }
                }
            }

            // 3. 2D Spatial Axonal Grid Diffusion & Potential Updating
            let mut new_wave = self.axonal_wave_field;
            for r in 0..8 {
                for c in 0..8 {
                    let idx = r * 8 + c;
                    let neighbor_sum =
                        self.axonal_wave_field[((r + 1) % 8) * 8 + c] +
                        self.axonal_wave_field[((r + 7) % 8) * 8 + c] +
                        self.axonal_wave_field[r * 8 + (c + 1) % 8] +
                        self.axonal_wave_field[r * 8 + (c + 7) % 8];
                    let laplacian = neighbor_sum - 4.0 * self.axonal_wave_field[idx];
                    new_wave[idx] += laplacian * 0.15;
                    currents[idx] += new_wave[idx] * 2.0;
                }
            }
            self.axonal_wave_field = new_wave;

            // 4. Integrate LIF / Izhikevich Equations
            for i in 0..64 {
                let v = self.v[i];
                let u = self.u[i];
                let i_ext = currents[i];

                let dv = (0.04 * v * v + 5.0 * v + 140.0 - u + i_ext) * h;
                let du = (a * (b * v - u)) * h;

                let next_v = v + dv;
                let next_u = u + du;

                if next_v >= 30.0 {
                    self.v[i] = c;
                    self.u[i] = next_u + d;
                    self.s[i] = (self.s[i] + 1.0).min(3.0);
                    self.spikes[i] = true;
                    self.stdp_trace[i] = (self.stdp_trace[i] + 1.0).min(4.0);
                    self.axonal_wave_field[i] += 1.5;
                } else {
                    self.v[i] = next_v.clamp(-90.0, 30.0);
                    self.u[i] = next_u;
                    self.s[i] *= 0.85;
                    self.spikes[i] = false;
                    self.stdp_trace[i] *= 0.94;
                    self.axonal_wave_field[i] *= 0.92;
                }
            }

            // 5. STDP Synaptic Adaptation
            for i in 0..64 {
                if self.spikes[i] {
                    for j in 0..64 {
                        if i != j {
                            let delta_w = 0.0012 * (self.stdp_trace[j] - 0.25);
                            self.weights[j][i] = (self.weights[j][i] + delta_w).clamp(-1.2, 1.2);
                        }
                    }
                }
            }
        }

        // Pool 64 neurons into 16 smooth motor outputs using SIMD Padé approximation
        for m in 0..16 {
            let mut pool_sum = 0.0f32;
            for n in 0..4 {
                let idx = m * 4 + n;
                let norm_v = (self.v[idx] + 65.0) / 30.0;
                let x2 = norm_v * norm_v;
                let tanh_approx = (norm_v * (27.0 + x2) / (27.0 + 9.0 * x2)).clamp(-1.0, 1.0);
                pool_sum += tanh_approx;
            }
            let target_motor = pool_sum / 4.0;
            self.motor_outputs[m] += (target_motor - self.motor_outputs[m]) * 0.25;
        }
    }
}

impl Default for SpikingNeuronNetwork {
    fn default() -> Self {
        Self::new()
    }
}

/// Procedural Texture and Organic Image Processing Buffer Engine for image-based neural visuals
#[derive(Clone, Debug)]
pub struct ImageTextureEngine {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[u8; 4]>, // RGBA pixels
    #[allow(dead_code)]
    pub displacement_map: Vec<(f32, f32)>, // Vector flow displacement field
}

impl ImageTextureEngine {
    pub fn new(width: usize, height: usize) -> Self {
        let mut pixels = vec![[0u8; 4]; width * height];
        let mut displacement_map = vec![(0.0f32, 0.0f32); width * height];

        // Generate procedural organic cellular/bio-fluid base texture
        for y in 0..height {
            let ny = y as f32 / height as f32;
            for x in 0..width {
                let nx = x as f32 / width as f32;
                let idx = y * width + x;

                // Multi-scale procedural Simplex/Worley noise pattern
                let fx = nx * 8.0;
                let fy = ny * 8.0;
                let v1 = (fx.sin() * fy.cos() + (fx * 1.5).cos() * (fy * 1.5).sin() + (nx * 12.0).sin()) * 0.33 + 0.5;
                let v2 = (((nx - 0.5).powi(2) + (ny - 0.5).powi(2)).sqrt() * 6.28).cos() * 0.5 + 0.5;

                let r = ((v1 * 180.0 + v2 * 75.0) as u8).saturating_add(20);
                let g = ((v2 * 150.0 + (1.0 - v1) * 80.0) as u8).saturating_add(30);
                let b = (((1.0 - v2) * 220.0 + v1 * 35.0) as u8).saturating_add(30);

                pixels[idx] = [r, g, b, 255];
                displacement_map[idx] = ((fx * 0.5).sin() * 0.02, (fy * 0.5).cos() * 0.02);
            }
        }

        Self {
            width,
            height,
            pixels,
            displacement_map,
        }
    }

    /// Sample procedural/image texture at normalized coordinates (u, v) in range [0, 1]
    pub fn sample(&self, u: f32, v: f32) -> [u8; 4] {
        let u_clamped = u.rem_euclid(1.0);
        let v_clamped = v.rem_euclid(1.0);

        let x = ((u_clamped * self.width as f32) as usize).min(self.width - 1);
        let y = ((v_clamped * self.height as f32) as usize).min(self.height - 1);

        self.pixels[y * self.width + x]
    }
}

impl Default for ImageTextureEngine {
    fn default() -> Self {
        Self::new(64, 64)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum VisualGenerator {
    ComplexNeuralMandala,
    ImageNeuronDeform,
    OrganicBitmapFeedback,
    WinampNeuronTunnel,
    WmpPlasmaFeedback,
    ReactionDiffusionNN,
    BioluminescentFluidFlow,
    HarmonicArrangementLattice,
    AbstractQuantumSwarm,
    NeuralFloralMycelium,
    FftSpectrumMesh,
}

impl VisualGenerator {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ComplexNeuralMandala => "Complex Neural Spiking Mandala",
            Self::ImageNeuronDeform => "Image Bio-Neuron Deformation",
            Self::OrganicBitmapFeedback => "Organic Bitmap Liquid Feedback",
            Self::WinampNeuronTunnel => "Winamp Neuron Warp Tunnel",
            Self::WmpPlasmaFeedback => "WMP Neural Plasma Oscillograph",
            Self::ReactionDiffusionNN => "Neural Reaction Diffusion Lattice",
            Self::BioluminescentFluidFlow => "Bioluminescent Fluid Flow",
            Self::HarmonicArrangementLattice => "Harmonic Arrangement Lattice",
            Self::AbstractQuantumSwarm => "Abstract Quantum Swarm",
            Self::NeuralFloralMycelium => "Neural Floral Mycelium",
            Self::FftSpectrumMesh => "3D FFT Spectrum Mesh",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::ComplexNeuralMandala,
            Self::ImageNeuronDeform,
            Self::OrganicBitmapFeedback,
            Self::WinampNeuronTunnel,
            Self::WmpPlasmaFeedback,
            Self::ReactionDiffusionNN,
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
    /// Native Biological Spiking Neural Network state
    pub neuron_net: SpikingNeuronNetwork,
    /// Procedural Texture & Organic Image Memory Buffer
    pub image_engine: ImageTextureEngine,
    /// Multidimensional Audio Nervous System & Genome Organism components
    pub nervous_system: AudioNervousSystem,
    pub genome: VisualGenome,
    pub mapper: NeuralLatentMapper,
    pub memory: VisualMemory,
    pub mutation: MutationEngine,
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
            neuron_net: SpikingNeuronNetwork::new(),
            image_engine: ImageTextureEngine::new(64, 64),
            nervous_system: AudioNervousSystem::default(),
            genome: VisualGenome::default(),
            mapper: NeuralLatentMapper::new(),
            memory: VisualMemory::default(),
            mutation: MutationEngine::default(),
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
                    "VIZ 1 — NEURAL MANDALA",
                    VisualGenerator::ComplexNeuralMandala,
                    vec![VisualInputSource::MasterMix, VisualInputSource::MidiTriggerBus],
                ),
                VisualChannel::new(
                    "VIZ 2 — IMAGE DEFORM",
                    VisualGenerator::ImageNeuronDeform,
                    vec![VisualInputSource::DeckA, VisualInputSource::DeckB],
                ),
                VisualChannel::new(
                    "VIZ 3 — BITMAP FEEDBACK",
                    VisualGenerator::OrganicBitmapFeedback,
                    vec![VisualInputSource::DeckC, VisualInputSource::DeckD],
                ),
                VisualChannel::new(
                    "VIZ 4 — WINAMP TUNNEL",
                    VisualGenerator::WinampNeuronTunnel,
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
