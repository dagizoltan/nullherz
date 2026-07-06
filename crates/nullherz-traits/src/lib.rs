#[cfg(feature = "test-utils")]
pub mod test_kit;
pub mod telemetry;
pub mod error;

use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    pub sample_rate: f32,
    pub block_size: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Transport {
    pub bpm: f32,
    pub beat_position: f64,
    pub is_playing: bool,
    pub sample_rate: f32,
    pub absolute_samples: u64,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ProcessorTypeId(pub u32);

impl ProcessorTypeId {
    pub const BIQUAD: Self = Self(1);
    pub const GAIN: Self = Self(2);
    pub const SAMPLER: Self = Self(10);
    pub const BIQUAD_EQ: Self = Self(11);
    pub const CROSSFADER: Self = Self(20);
    pub const SUMMING: Self = Self(30);
    pub const SPECTRAL: Self = Self(40);
    pub const WAVETABLE: Self = Self(50);
    pub const MODULATION: Self = Self(60);
    pub const SEQUENCER: Self = Self(70);
    pub const ENVELOPE_FOLLOWER: Self = Self(80);
    pub const GRANULAR: Self = Self(90);
    pub const SPECTRAL_MORPH: Self = Self(100);
    pub const CAPTURE: Self = Self(110);
    pub const DJ_ISOLATOR: Self = Self(120);
    pub const SIMD_BIQUAD: Self = Self(130);
    pub const KEY_SYNC: Self = Self(140);
    pub const PERSONALITY_INHERITANCE: Self = Self(150);
}

impl From<u32> for ProcessorTypeId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<ProcessorTypeId> for u32 {
    fn from(id: ProcessorTypeId) -> Self {
        id.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u32)]
pub enum DeckParamType {
    Gain,
    EqLow,
    EqMid,
    EqHigh,
    Filter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u32)]
pub enum AudioBackendType {
    Alsa,
    Pipewire,
    Jack,
    Threaded,
    Mock,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CoreCommand {
    Play,
    Stop,
    SetSafeMode(bool),
    RequestSnapshots,
    CommitTopology,
    SetBpm(f32),
    SwitchBackend(AudioBackendType),
    CalibrateLatency,
    #[serde(with = "serde_big_array::BigArray")]
    LoadMidiMap([u8; 32]), // Fixed-size buffer for filename
    #[serde(with = "serde_big_array::BigArray")]
    SetMidiPorts([u8; 128]), // Comma-separated list or similar
    HotLoadSidecar {
        #[serde(with = "serde_big_array::BigArray")]
        name: [u8; 32],
        node_idx: u32,
    },
    ExportAudio {
        #[serde(with = "serde_big_array::BigArray")]
        filename: [u8; 64],
        duration_seconds: f32,
    },
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MixerCommand {
    SetParam {
        target_id: u64,
        param_id: u32,
        value: f32,
        ramp_duration_samples: u32,
    },
    Bundle {
        count: u32,
        #[serde(with = "serde_big_array::BigArray")]
        data: [u8; 128],
    },
    SetMacro {
        macro_id: u32,
        value: f32,
    },
    AddModMapping {
        macro_id: u32,
        target_id: u64,
        param_id: u32,
        scaling: f32,
        ramp_duration_samples: u32,
    },
    RemoveModMapping {
        macro_id: u32,
        target_id: u64,
        param_id: u32,
    },
    SetDeckParam {
        deck_id: char,
        param_type: DeckParamType,
        value: f32,
    },
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PerformanceCommand {
    SetSequencerStep {
        node_idx: u32,
        track: u32,
        step: u32,
        value: bool,
    },
    JumpToHotCue {
        node_idx: u32,
        cue_idx: u32,
    },
    JumpByBeats {
        node_idx: u32,
        beats: f32,
    },
    SetLoop {
        node_idx: u32,
        enabled: bool,
        start_samples: u64,
        end_samples: u64,
    },
    SetSlipMode {
        node_idx: u32,
        enabled: bool,
    },
    TriggerSlice {
        node_idx: u32,
        slice_idx: u32,
    },
    LaunchClip {
        row: u32,
        col: u32,
    },
    TransfuseRow {
        row: u32,
    },
    LoadTrackToDeck {
        deck_id: char,
        sample_id: u64,
    },
    SyncDecks {
        source_deck: char,
        target_deck: char,
    },
    PlayDeck {
        deck_id: char,
    },
    StopDeck {
        deck_id: char,
    },
    EvolvePattern {
        node_idx: u32,
        track_idx: u32,
        mutation_strength: f32,
    },
    SetTrackMute {
        node_idx: u32,
        track_idx: u32,
        muted: bool,
    },
    SetTrackSolo {
        node_idx: u32,
        track_idx: u32,
        soloed: bool,
    },
    ClearTrackPattern {
        node_idx: u32,
        track_idx: u32,
    },
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ResourceCommand {
    RegisterCapture {
        capture_node_idx: u32,
        sample_id: u64,
    },
    AddSourceFromRegistry {
        granular_node_idx: u32,
        sample_id: u64,
    },
    CommitBreeding {
        parent_a_id: u64,
        parent_b_id: u64,
        bias: f32,
    },
    CommitChaoticBreeding {
        parent_a_id: u64,
        parent_b_id: u64,
        bias: f32,
        chaotic_strength: f32,
    },
    ApplyFeatureMutation {
        target_id: u64,
        #[serde(with = "serde_big_array::BigArray")]
        feature_name: [u8; 32],
        strength: f32,
    },
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpaqueEnvelope {
    pub domain_id: u32,
    pub target_id: u64,
    pub opcode: u32,
    pub data: [u8; 32],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DnaCommand {
    pub target_id: u64,
    pub layer_mask: u32,
    pub bias: f32,
    #[serde(with = "serde_big_array::BigArray")]
    pub payload: [u8; 128],
}

impl DnaCommand {
    /// Zero-allocation, type-safe builder for DNA transfusion payloads.
    pub fn pack_transfusion(target_id: u64, latent: &[f32; 16], micro_timing: &[i16; 12], onset_mask: &[u64; 4]) -> Self {
        let mut payload = [0u8; 128];

        // 1. Spectral (0-63)
        unsafe {
            std::ptr::copy_nonoverlapping(latent.as_ptr() as *const u8, payload.as_mut_ptr(), 64);
        }

        // 2. Rhythmic Micro-timing (64-75)
        for i in 0..12 {
            payload[64 + i] = (micro_timing[i] as i8) as u8;
        }

        // 3. Rhythmic Onset Mask (76-107)
        for i in 0..4 {
            let mask = onset_mask[i];
            for j in 0..8 {
                payload[76 + i * 8 + j] = ((mask >> (j * 8)) & 0xFF) as u8;
            }
        }

        Self {
            target_id,
            layer_mask: 3, // Spectral + Rhythmic
            bias: 1.0,
            payload,
        }
    }
}

/// Represents an action to be performed by the audio engine.
/// Refactored into a modular hierarchy to ensure ABI stability and decoupling.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    Core(CoreCommand),
    Mixer(MixerCommand),
    Performance(PerformanceCommand),
    Topology(TopologyCommand),
    Resource(ResourceCommand),
    Extension(OpaqueEnvelope),
    Dna(DnaCommand),
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TopologyCommand {
    AddNode {
        processor_type_id: ProcessorTypeId,
        node_idx: u32,
    },
    UpdateEdge {
        node_idx: u32,
        input_idx: u32,
        new_buffer_idx: u32,
    },
    UpdateOutputEdge {
        node_idx: u32,
        output_idx: u32,
        new_buffer_idx: u32,
    },
    SwapProcessor {
        node_idx: u32,
        processor_type_id: ProcessorTypeId,
    },
    SetBypass {
        node_idx: u32,
        enabled: bool,
    },
}

/// A command with an associated timestamp for deterministic execution.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TimestampedCommand {
    pub timestamp_samples: u64,
    pub command: Command,
}

impl TimestampedCommand {
    pub fn to_binary(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(bincode::serialize(self)?)
    }

    pub fn from_binary(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(bincode::deserialize(data)?)
    }
}

impl SoundDNA {
    pub fn to_binary(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(bincode::serialize(self)?)
    }

    pub fn from_binary(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(bincode::deserialize(data)?)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ParameterMetadata {
    pub id: u32,
    pub name: [u8; 32],
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProcessorMetadata {
    pub processor_id: u64,
    pub num_parameters: u32,
    pub parameters: [ParameterMetadata; 16],
}

#[derive(Clone, Copy, Debug)]
pub struct CompiledGraphPlan {
    pub stages: [[usize; MAX_NODES]; MAX_NODES],
    pub stage_counts: [usize; MAX_NODES],
    pub num_stages: usize,
}

impl Default for CompiledGraphPlan {
    fn default() -> Self {
        Self {
            stages: [[0; MAX_NODES]; MAX_NODES],
            stage_counts: [0; MAX_NODES],
            num_stages: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NodeRouting {
    pub input_indices: [usize; MAX_CHANNELS],
    pub output_indices: [usize; MAX_CHANNELS],
    pub input_count: usize,
    pub output_count: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct CrossfadeState {
    pub node_idx: usize,
    pub input_idx: usize,
    pub old_buffer_idx: usize,
    pub new_buffer_idx: usize,
    pub remaining_samples: u32,
    pub total_samples: u32,
}

#[derive(Clone, Debug)]
pub struct GraphTopology {
    pub routing: [NodeRouting; MAX_NODES],
    pub virtual_to_physical: [usize; MAX_NODES],
    pub plan: CompiledGraphPlan,
    pub crossfades: [Option<CrossfadeState>; 8],
    pub node_count: usize,
    pub node_assignments: std::collections::HashMap<u32, String>, // node_idx -> "local" or sidecar addr
}

pub enum TopologyMutation {
    UpdateEdge {
        node_idx: u32,
        input_idx: u32,
        new_buffer_idx: u32,
    },
    UpdateOutputEdge {
        node_idx: u32,
        output_idx: u32,
        new_buffer_idx: u32,
    },
    SwapProcessor {
        node_idx: u32,
        processor: Box<dyn AudioProcessor>,
    },
    AddNode {
        node_idx: u32,
        processor: Box<dyn AudioProcessor>,
    },
    AddSource {
        node_idx: u32,
        buffer: Arc<Vec<f32>>,
        sample_id: u64,
        metadata: Option<Arc<SampleMetadata>>,
    },
    UpdateMetadata {
        node_idx: u32,
        metadata: Arc<SampleMetadata>,
    },
    LoadProcessorState {
        node_idx: u32,
        state_data: Arc<Vec<u8>>,
    },
    SetTopology(Arc<GraphTopology>),
}

/// Interface for processors to interact with the engine host (e.g., scheduling commands).
pub trait Host: Send + Sync + 'static {
    /// Pushes a command to be executed by the engine at a specific timestamp.
    fn push_command(&self, timestamp_samples: u64, command: Command);

    /// Requests the host to pull a snapshot from a processor and register it.
    fn request_registration(&self, capture_node_idx: u32, sample_id: u64);
}

/// Shared execution context passed to processors during the audio block cycle.
pub struct ProcessContext<'a> {
    /// Global transport information (BPM, position, play state).
    pub transport: Option<&'a Transport>,
    /// Interface to the engine host.
    pub host: Option<&'a dyn Host>,
    /// Current sample offset within the physical audio block (used for sample-accurate automation).
    pub sub_block_offset: usize,
    /// Flag indicating if this is the final sub-block for the current engine cycle.
    pub is_last_sub_block: bool,
}

pub trait ParallelExecutor: Send + Sync {
    fn as_any(&mut self) -> &mut dyn std::any::Any;
    fn num_workers(&self) -> usize;
    fn push_job(&mut self, worker_idx: usize, job: Box<dyn std::any::Any + Send>) -> Result<(), Box<dyn std::any::Any + Send>>;
    fn wait_for_completion(&mut self, target_count: usize);
    fn current_completion_count(&self) -> usize;
    fn notify_workers(&mut self);
}

pub trait ExecutionProvider: Send + Sync {
    fn as_any(&mut self) -> &mut dyn std::any::Any;
}

/// Alignment for SIMD (AVX-512 requires 64 bytes).
pub const SIMD_ALIGNMENT: usize = 64;
pub const MAX_BLOCK_SIZE: usize = 256;
pub const MAX_NODES: usize = 64;
pub const MAX_CHANNELS: usize = 16;
pub const MAX_CROSSFADE_BUFFERS: usize = 8;
pub const MAX_MUTATIONS: usize = 16;
pub const DEFAULT_WORKER_COUNT: usize = 4;
pub const MAX_COMMANDS_PER_BLOCK: usize = 256;

pub struct SubBlock {
    pub offset: usize,
    pub len: usize,
    pub is_last: bool,
}

pub struct SubBlockIterator {
    pub total_len: usize,
    pub max_block_size: usize,
    pub current_offset: usize,
}

impl SubBlockIterator {
    pub fn new(total_len: usize, max_block_size: usize) -> Self {
        Self { total_len, max_block_size, current_offset: 0 }
    }

    pub fn next_chunk(&mut self) -> Option<SubBlock> {
        if self.current_offset >= self.total_len { return None; }
        let remaining = self.total_len - self.current_offset;
        let len = remaining.min(self.max_block_size);
        let block = SubBlock {
            offset: self.current_offset,
            len,
            is_last: (self.current_offset + len) == self.total_len,
        };
        self.current_offset += len;
        Some(block)
    }

    pub fn next_chunk_up_to(&mut self, end_offset: usize) -> Option<SubBlock> {
        let end_limit = end_offset.min(self.total_len);
        if self.current_offset >= end_limit { return None; }
        let remaining = end_limit - self.current_offset;
        let len = remaining.min(self.max_block_size);
        let block = SubBlock {
            offset: self.current_offset,
            len,
            is_last: (self.current_offset + len) == self.total_len,
        };
        self.current_offset += len;
        Some(block)
    }
}

/// A SIMD-aligned audio block.
#[repr(C, align(64))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AudioBlock {
    pub data: [f32; MAX_BLOCK_SIZE],
    pub len: u32,
    pub _pad: [u32; 15], // Pad to 64-byte alignment (1024 + 4 + 60 = 1088)
}

/// A standard MIDI event representation for real-time IPC.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiEvent {
    pub timestamp_samples: u64,
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
    pub _pad: u8,
}

/// Interface for real-time safe deallocation of processors.
pub trait GarbageProducer: Send + dyn_clone::DynClone {
    fn push_processor(&mut self, processor: Box<dyn AudioProcessor>) -> Result<(), Box<dyn AudioProcessor>>;
}

dyn_clone::clone_trait_object!(GarbageProducer);

/// Command interface for processors to decouple from the control plane.
pub type ProcessorCommand = Command;

/// Marker trait for real-time safe components.
/// Types implementing this trait guarantee that their methods do not perform
/// heap allocations, take locks, or execute blocking syscalls.
pub trait RtSafe {}

pub trait ProcessorFactory: Send + Sync {
    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>>;
    fn name(&self) -> &'static str;
}

pub trait SignalProcessor: Send {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext);
    fn process_parallel(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext, _executor: Option<&mut (dyn ParallelExecutor + '_)>) {
        self.process(inputs, outputs, context);
    }
    fn setup(&mut self, _config: AudioConfig) {}
    fn set_safe_mode(&mut self, _enabled: bool) {}
    fn reset(&mut self) {}
    fn latency_samples(&self) -> usize { 0 }
}

pub trait MidiResponder: Send {
    fn apply_midi(&mut self, _event: MidiEvent, _context: Option<&ProcessContext>) {}
}

pub trait SnapshotProvider: Send {
    fn pull_snapshot(&mut self) -> Option<Arc<Vec<f32>>> { None }
    fn pull_all_snapshots(&mut self, _target: &mut Vec<(u64, Arc<Vec<f32>>)>) {}
}

pub trait AudioProcessor: SignalProcessor + MidiResponder + SnapshotProvider + Send {
    fn apply_command(&mut self, _command: &ProcessorCommand) {}
    fn set_parameter(&mut self, _param_id: u32, _value: f32, _ramp_duration_samples: u32) {}
    fn get_parameter(&self, _param_id: u32) -> f32 { 0.0 }
    fn serialize_state(&self) -> Vec<u8> { Vec::new() }
    fn apply_topology_mutation(&mut self, _mutation: TopologyMutation) {}
    fn collect_telemetry(&self, _node_times: &mut [u64; MAX_NODES], _peak_levels: &mut [f32; MAX_NODES]) {}
    fn metadata(&self) -> Option<ProcessorMetadata> { None }
    fn set_garbage_producer(&mut self, _producer: Box<dyn GarbageProducer>) {}
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
    fn list_children(&self) -> Vec<&dyn AudioProcessor> { Vec::new() }
    fn resource_id(&self) -> Option<u64> { None }
    fn load_state(&mut self, _data: &[u8]) {}
}

pub trait CommandProducer: Send + Sync + dyn_clone::DynClone {
    fn push_command(&self, command: TimestampedCommand) -> Result<(), Command>;
}

dyn_clone::clone_trait_object!(CommandProducer);

pub trait CommandConsumer: Send {
    fn pop_command(&mut self) -> Option<TimestampedCommand>;
}

pub trait TelemetryProducer: Send {
    fn push_telemetry(&mut self, telemetry: crate::telemetry::Telemetry) -> Result<(), crate::telemetry::Telemetry>;
}

pub trait MidiConsumer: Send {
    fn pop(&mut self) -> Option<MidiEvent>;
}

pub trait TopologyMutationConsumer: Send {
    fn pop(&mut self) -> Option<TopologyMutation>;
}

pub trait CommandBundleConsumer: Send {
    fn pop(&mut self) -> Option<Vec<Command>>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SpectralPersonality {
    /// 16-float latent-space representation of the spectral personality
    pub latent_space: [f32; 16],
    /// Ratio of periodic vs aperiodic energy across 8 octaves (16 bits per octave)
    pub harmonicity: [u16; 8],
    /// Spectral slope/tilt
    pub tilt: f32,
    /// Top 5 resonant peaks: (Freq, Q, Gain)
    pub formant_peaks: [(f32, u16, u16); 5],
}

impl Default for SpectralPersonality {
    fn default() -> Self {
        Self {
            latent_space: [0.0; 16],
            harmonicity: [0; 8],
            tilt: 0.0,
            formant_peaks: [(0.0, 0, 0); 5],
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RhythmicDNA {
    /// 64-step bitmask indicating significant transient density over 4 bars
    pub onset_mask: [u64; 4],
    /// Measure of rhythmic complexity
    pub syncopation_index: f32,
    /// Deviation profile from absolute grid (Early/Late bias)
    pub micro_timing: [i16; 12],
}

impl Default for RhythmicDNA {
    fn default() -> Self {
        Self {
            onset_mask: [0; 4],
            syncopation_index: 0.0,
            micro_timing: [0; 12],
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ArtifactProfile {
    pub aliasing_threshold: f32,
    pub noise_floor_db: f32,
    pub glitch_density: f32,
}

impl Default for ArtifactProfile {
    fn default() -> Self {
        Self {
            aliasing_threshold: 1.0,
            noise_floor_db: -96.0,
            glitch_density: 0.0,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SpatialDNA {
    pub stereo_width: f32,
    pub room_size: f32,
    /// Delay offsets of the first 8 reflection taps in ms
    pub er_taps: [f32; 8],
    /// Gain of the first 8 reflection taps
    pub er_gains: [f32; 8],
}

impl Default for SpatialDNA {
    fn default() -> Self {
        Self {
            stereo_width: 1.0,
            room_size: 0.0,
            er_taps: [0.0; 8],
            er_gains: [0.0; 8],
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SoundDNA {
    pub schema_version: u16,
    pub feature_vector: [f32; 8],
    pub spectral: SpectralPersonality,
    pub rhythmic: RhythmicDNA,
    pub artifacts: ArtifactProfile,
    pub spatial: SpatialDNA,
}

impl Default for SoundDNA {
    fn default() -> Self {
        Self {
            schema_version: 6,
            feature_vector: [0.0; 8],
            spectral: SpectralPersonality::default(),
            rhythmic: RhythmicDNA::default(),
            artifacts: ArtifactProfile::default(),
            spatial: SpatialDNA::default(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct MipWaveform {
    /// Level 0 is full resolution peaks.
    /// Subsequent levels are downsampled by powers of 2.
    #[serde(skip)]
    pub levels: Vec<Arc<Vec<f32>>>,
}

impl Default for MipWaveform {
    fn default() -> Self {
        Self { levels: vec![] }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SampleMetadata {
    pub bpm: f32,
    #[serde(skip)]
    pub transients: Arc<Vec<u64>>,
    pub root_key: Option<f32>,
    pub hot_cues: [Option<u64>; 8],
    pub loop_points: Option<(u64, u64)>,
    pub beat_grid_offset: u64,
    #[serde(skip)]
    pub peaks: Arc<Vec<f32>>,
    pub mip_waveform: MipWaveform,
    pub dna: SoundDNA,
    pub midi_map: Option<MidiMap>,
}

impl SampleMetadata {
    pub fn new_empty() -> Self {
        Self {
            bpm: 0.0,
            transients: Arc::new(Vec::new()),
            root_key: None,
            hot_cues: [None; 8],
            loop_points: None,
            beat_grid_offset: 0,
            peaks: Arc::new(Vec::new()),
            mip_waveform: MipWaveform::default(),
            dna: SoundDNA::default(),
            midi_map: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum MidiTarget {
    Param { target_id: u64, param_id: u32 },
    Macro { macro_id: u32 },
    Command(Command),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ControlMapping {
    pub cc_number: u8,
    pub target: MidiTarget,
    pub min_val: f32,
    pub max_val: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TriggerMapping {
    pub note_number: u8,
    pub target: MidiTarget,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Default)]
pub struct MidiMap {
    pub name: String,
    pub controls: Vec<ControlMapping>,
    pub triggers: Vec<TriggerMapping>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SidecarManifest {
    pub name: String,
    pub version: String,
    pub author: String,
    pub processor_type_id: u32,
    pub binary_name: String,
}

pub trait ProcessingKernel: Send {
    #[allow(clippy::too_many_arguments)]
    fn execute(
        &mut self,
        graph: &mut dyn AudioProcessor,
        transport: &mut Transport,
        host: Option<&dyn Host>,
        pool: &mut Option<Box<dyn ParallelExecutor>>,
        command_consumer: &mut Box<dyn CommandConsumer>,
        pending_command: &mut Option<TimestampedCommand>,
        sample_counter: u64,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        num_samples: usize,
    );
}

/// Abstract interface for the audio rendering engine.
/// This allows backends to be decoupled from the concrete `AudioEngine` implementation.
pub trait RenderingEngine: Send + Sync {
    /// Process a block of audio. This is the primary entry point for audio processing.
    fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize);
    /// Update the engine configuration (sample rate, block size).
    fn set_config(&mut self, config: AudioConfig);
    /// Returns the target sample rate configured for the engine.
    fn target_sample_rate(&self) -> f32;
    /// Pulls all available snapshots from the signal graph for registration.
    fn pull_all_snapshots(&self, target: &mut Vec<(u64, Arc<Vec<f32>>)>);
    /// Returns a list of all currently active child processors.
    fn list_children(&self) -> Vec<&dyn AudioProcessor>;
}

/// Interface for controlling the audio engine from a non-RT thread.
pub trait RenderingController: Send + Sync {
    /// Schedules a new signal graph to be swapped in at the next block boundary.
    fn set_pending_graph(&self, graph: Box<dyn AudioProcessor>);
}

pub struct BundleIterator<'a> {
    data: &'a [u8; 128],
    count: usize,
    index: usize,
}

impl<'a> Iterator for BundleIterator<'a> {
    type Item = Command;
    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.count || self.index >= 8 { return None; }
        let offset = self.index * 16;
        let target_id = u64::from_le_bytes(self.data[offset..offset+8].try_into().unwrap());
        let param_id = u32::from_le_bytes(self.data[offset+8..offset+12].try_into().unwrap());
        let value = f32::from_le_bytes(self.data[offset+12..offset+16].try_into().unwrap());
        self.index += 1;
        Some(Command::Mixer(MixerCommand::SetParam {
            target_id,
            param_id,
            value,
            ramp_duration_samples: 0
        }))
    }
}

impl Command {
    pub fn bundle_iter(&self) -> Option<BundleIterator<'_>> {
        if let Command::Mixer(MixerCommand::Bundle { count, data }) = self {
            Some(BundleIterator { data, count: *count as usize, index: 0 })
        } else {
            None
        }
    }

    #[deprecated(note = "Use bundle_iter instead to avoid allocation")]
    pub fn unpack_bundle(count: u32, data: [u8; 128]) -> Vec<Command> {
        let mut commands = Vec::with_capacity(count as usize);
        let iter = BundleIterator { data: &data, count: count as usize, index: 0 };
        for cmd in iter {
            commands.push(cmd);
        }
        commands
    }
}
pub struct IdAllocator {
    next_node_id: std::sync::atomic::AtomicU32,
    next_buffer_id: std::sync::atomic::AtomicU32,
}
impl IdAllocator {
    pub fn new(start_node_id: u32, start_buffer_id: u32) -> Self {
        Self { next_node_id: std::sync::atomic::AtomicU32::new(start_node_id), next_buffer_id: std::sync::atomic::AtomicU32::new(start_buffer_id) }
    }
    pub fn allocate_node_id(&self) -> u32 { self.next_node_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed) }
    pub fn allocate_buffer_id(&self, count: u32) -> u32 { self.next_buffer_id.fetch_add(count, std::sync::atomic::Ordering::Relaxed) }
    pub fn current_node_id(&self) -> u32 { self.next_node_id.load(std::sync::atomic::Ordering::Relaxed) }
    pub fn current_buffer_id(&self) -> u32 { self.next_buffer_id.load(std::sync::atomic::Ordering::Relaxed) }
}
impl Default for IdAllocator { fn default() -> Self { Self::new(0, 12) } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mip_waveform_default() {
        let mip = MipWaveform::default();
        assert!(mip.levels.is_empty());
    }

    #[test]
    fn test_binary_serialization() {
        let cmd = TimestampedCommand {
            timestamp_samples: 1234,
            command: Command::Core(CoreCommand::Play),
        };
        let binary = cmd.to_binary().unwrap();
        let decoded = TimestampedCommand::from_binary(&binary).unwrap();
        assert_eq!(cmd, decoded);

        let mut dna = SoundDNA::default();
        dna.spectral.latent_space[0] = 1.0;
        let dna_binary = dna.to_binary().unwrap();
        let dna_decoded = SoundDNA::from_binary(&dna_binary).unwrap();
        assert_eq!(dna, dna_decoded);
    }
}
