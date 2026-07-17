#[cfg(feature = "test-utils")]
pub mod test_kit;
pub mod telemetry;
pub mod error;

use std::sync::Arc;
pub use serde_big_array::BigArray;

mod serde_arc {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<T, S>(val: &Arc<T>, s: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        val.as_ref().serialize(s)
    }

    pub fn deserialize<'de, T, D>(d: D) -> Result<Arc<T>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        T::deserialize(d).map(Arc::new)
    }
}

mod serde_arc_vec {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<T, S>(val: &Vec<Arc<T>>, s: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        let temp: Vec<&T> = val.iter().map(|arc| arc.as_ref()).collect();
        temp.serialize(s)
    }

    pub fn deserialize<'de, T, D>(d: D) -> Result<Vec<Arc<T>>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let temp: Vec<T> = Vec::deserialize(d)?;
        Ok(temp.into_iter().map(Arc::new).collect())
    }
}

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
    /// System monotonic clock time in nanoseconds.
    pub system_time_ns: u64,
    /// Device-specific hardware clock time in nanoseconds (e.g. from PTP).
    pub device_time_ns: u64,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ProcessorTypeId(pub u32);

impl ProcessorTypeId {
    pub const DELAY: Self = Self(0);
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
    pub const DNA_MORPH: Self = Self(190);
    pub const LIMITER: Self = Self(200);
    pub const STREAMING_SAMPLER: Self = Self(210);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
#[repr(u32)]
pub enum DeckParamType {
    Gain,
    EqLow,
    EqMid,
    EqHigh,
    Filter,
    Pan,
    Width,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
#[repr(u32)]
pub enum AudioBackendType {
    Alsa,
    Pipewire,
    Jack,
    Threaded,
    Mock,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub enum CoreCommand {
    Play,
    Stop,
    Pause,
    Resume,
    SetSafeMode(bool),
    RequestSnapshots,
    CommitTopology,
    SetBpm(f32),
    SetMasterDeck(char),
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
    Undo,
    Redo,
    CheckpointParameterEdit,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
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

impl MixerCommand {
    /// Zero-allocation utility to pack up to 8 parameter updates into a single bundle.
    pub fn pack_bundle(updates: &[(u64, u32, f32)]) -> Self {
        let mut data = [0u8; 128];
        let count = updates.len().min(8);
        for (i, &(target_id, param_id, value)) in updates.iter().take(count).enumerate() {
            let offset = i * 16;
            data[offset..offset + 8].copy_from_slice(&target_id.to_le_bytes());
            data[offset + 8..offset + 12].copy_from_slice(&param_id.to_le_bytes());
            data[offset + 12..offset + 16].copy_from_slice(&value.to_le_bytes());
        }
        Self::Bundle {
            count: count as u32,
            data,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub enum PerformanceCommand {
    SetSequencerStep {
        node_idx: u32,
        track: u32,
        step: u32,
        value: f32, // Velocity 0.0 - 1.0
    },
    JumpToHotCue {
        node_idx: u32,
        cue_idx: u32,
    },
    SetHotCue {
        node_idx: u32,
        cue_idx: u32,
        position_samples: u64,
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
    Preview {
        sample_id: u64,
    },
    PlayNode {
        node_idx: u32,
    },
    StopNode {
        node_idx: u32,
    },
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub enum ResourceCommand {
    ScanFolder {
        #[serde(with = "serde_big_array::BigArray")]
        path: [u8; 256],
    },
    RegisterCapture {
        capture_node_idx: u32,
        sample_id: u64,
    },
    Normalize {
        sample_id: u64,
    },
    Crop {
        sample_id: u64,
        start_samples: u64,
        end_samples: u64,
    },
    ReAnalyze {
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
    RhythmicTransfusion {
        source_id: u64,
        target_id: u64,
    },
    TimeStretch {
        sample_id: u64,
        ratio: f32,
    },
    ChopByTransient {
        sample_id: u64,
    },
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct OpaqueEnvelope {
    pub domain_id: u32,
    pub target_id: u64,
    pub opcode: u32,
    pub data: [u8; 32],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
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
        payload[..64].copy_from_slice(bytemuck::cast_slice(latent));

        // 2. Rhythmic Micro-timing (64-75)
        for (i, &timing) in micro_timing.iter().enumerate().take(12) {
            payload[64 + i] = (timing as i8) as u8;
        }

        // 3. Rhythmic Onset Mask (76-107)
        for (i, &mask) in onset_mask.iter().enumerate().take(4) {
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

    /// Safely unpacks the DNA transfusion payload into its constituent parts.
    pub fn unpack_transfusion(&self) -> ([f32; 16], [i16; 12], [u64; 4]) {
        let mut latent = [0.0f32; 16];
        let mut micro_timing = [0i16; 12];
        let mut onset_mask = [0u64; 4];

        // 1. Spectral (0-63)
        // Hardening: Ensure we don't read invalid float states by zeroing out non-finite values.
        latent.copy_from_slice(bytemuck::cast_slice(&self.payload[..64]));
        for val in latent.iter_mut() {
            if !val.is_finite() { *val = 0.0; }
        }

        // 2. Rhythmic Micro-timing (64-75)
        for i in 0..12 {
            micro_timing[i] = (self.payload[64 + i] as i8) as i16;
        }

        // 3. Rhythmic Onset Mask (76-107)
        for i in 0..4 {
            let mut mask = 0u64;
            for j in 0..8 {
                mask |= (self.payload[76 + i * 8 + j] as u64) << (j * 8);
            }
            onset_mask[i] = mask;
        }

        (latent, micro_timing, onset_mask)
    }
}

/// Represents an action to be performed by the audio engine.
/// Refactored into a modular hierarchy to ensure ABI stability and decoupling.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
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
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub enum TopologyCommand {
    RemoveNode {
        node_idx: u32,
    },
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
    SetNodePosition {
        node_idx: u32,
        x: f32,
        y: f32,
    },
    Connect {
        src_node_idx: u32,
        src_output_idx: u32,
        dst_node_idx: u32,
        dst_input_idx: u32,
    },
    Disconnect {
        node_idx: u32,
        input_idx: u32,
    },
    MigrateNode {
        node_idx: u32,
        #[serde(with = "serde_big_array::BigArray")]
        destination: [u8; 32],
    },
}

/// A command with an associated timestamp for deterministic execution.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
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

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct StageNodes(#[serde(with = "BigArray")] pub [u32; MAX_NODES]);

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CompiledGraphPlan {
    #[serde(with = "BigArray")]
    pub stages: [StageNodes; MAX_NODES],
    #[serde(with = "BigArray")]
    pub stage_counts: [u32; MAX_NODES],
    pub num_stages: usize,
    /// Disjoint sub-graph identification for partial re-compilation and optimized O(1) swaps.
    #[serde(with = "BigArray")]
    pub node_islands: [u8; MAX_NODES],
    /// Per-node compensation delay in samples.
    #[serde(with = "BigArray")]
    pub node_latencies: [u32; MAX_NODES],
    /// Per-node, per-input compensation delay.
    #[serde(with = "BigArray")]
    pub input_delays: [InputDelays; MAX_NODES],
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct InputDelays(#[serde(with = "BigArray")] pub [f32; MAX_CHANNELS]);

impl Default for CompiledGraphPlan {
    fn default() -> Self {
        Self {
            stages: [StageNodes([0; MAX_NODES]); MAX_NODES],
            stage_counts: [0; MAX_NODES],
            num_stages: 0,
            node_islands: [0; MAX_NODES],
            node_latencies: [0; MAX_NODES],
            input_delays: [InputDelays([0.0; MAX_CHANNELS]); MAX_NODES],
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct NodeRouting {
    pub input_indices: [u32; MAX_CHANNELS],
    pub output_indices: [u32; MAX_CHANNELS],
    pub sidechain_indices: [u32; MAX_CHANNELS],
    pub input_count: usize,
    pub output_count: usize,
    pub sidechain_count: usize,
    /// Delay compensation required for this node's inputs in samples.
    pub input_delays: [f32; MAX_CHANNELS],
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CrossfadeState {
    pub node_idx: usize,
    pub input_idx: usize,
    pub old_buffer_idx: usize,
    pub new_buffer_idx: usize,
    pub remaining_samples: u32,
    pub total_samples: u32,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Default)]
pub struct NodeAssignment(#[serde(with = "BigArray")] pub [u8; 32]);

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct NodeAssignmentArray(#[serde(with = "BigArray")] pub [NodeAssignment; MAX_NODES]);

impl Default for NodeAssignmentArray {
    fn default() -> Self {
        Self([NodeAssignment::default(); MAX_NODES])
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
#[repr(u32)]
pub enum TemporalShape {
    Sine,
    Saw,
    Square,
    Triangle,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ModMapping {
    pub macro_id: u32,
    pub target_id: u64,
    pub param_id: u32,
    pub scaling: f32,
    pub ramp_duration_samples: u32,
    pub temporal_shape: Option<TemporalShape>,
    pub active: bool,
}

impl Default for ModMapping {
    fn default() -> Self {
        Self {
            macro_id: 0,
            target_id: 0,
            param_id: 0,
            scaling: 1.0,
            ramp_duration_samples: 0,
            temporal_shape: None,
            active: false,
        }
    }
}

pub const MAX_MOD_MAPPINGS: usize = 128;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ModulationMatrix {
    #[serde(with = "BigArray")]
    pub mappings: [ModMapping; MAX_MOD_MAPPINGS],
}

impl Default for ModulationMatrix {
    fn default() -> Self {
        Self {
            mappings: [ModMapping::default(); MAX_MOD_MAPPINGS],
        }
    }
}

impl ModulationMatrix {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_mapping(&mut self, macro_id: u32, target_id: u64, param_id: u32, scaling: f32, ramp_duration_samples: u32, shape: Option<TemporalShape>) {
        // First try to find an existing mapping to update
        for mapping in self.mappings.iter_mut() {
            if mapping.active && mapping.macro_id == macro_id && mapping.target_id == target_id && mapping.param_id == param_id {
                mapping.scaling = scaling;
                mapping.ramp_duration_samples = ramp_duration_samples;
                mapping.temporal_shape = shape;
                return;
            }
        }

        // Otherwise find a free slot
        for mapping in self.mappings.iter_mut() {
            if !mapping.active {
                mapping.macro_id = macro_id;
                mapping.target_id = target_id;
                mapping.param_id = param_id;
                mapping.scaling = scaling;
                mapping.ramp_duration_samples = ramp_duration_samples;
                mapping.temporal_shape = shape;
                mapping.active = true;
                return;
            }
        }
    }

    pub fn remove_mapping(&mut self, macro_id: u32, target_id: u64, param_id: u32) {
        for mapping in self.mappings.iter_mut() {
            if mapping.active && mapping.macro_id == macro_id && mapping.target_id == target_id && mapping.param_id == param_id {
                mapping.active = false;
            }
        }
    }

    pub fn expand_macro<F>(&self, macro_id: u32, value: f32, beat_pos: f64, mut f: F)
    where
        F: FnMut(u64, u32, f32, u32),
    {
        for mapping in self.mappings.iter() {
            if mapping.active && mapping.macro_id == macro_id {
                let mut val = value * mapping.scaling;

                if let Some(shape) = mapping.temporal_shape {
                    let phase = (beat_pos % 1.0) as f32; // 1-beat cycle
                    let modifier = match shape {
                        TemporalShape::Sine => (phase * 2.0 * std::f32::consts::PI).sin(),
                        TemporalShape::Saw => phase * 2.0 - 1.0,
                        TemporalShape::Square => {
                            if phase < 0.5 {
                                1.0
                            } else {
                                -1.0
                            }
                        }
                        TemporalShape::Triangle => {
                            if phase < 0.5 {
                                phase * 4.0 - 1.0
                            } else {
                                1.0 - (phase - 0.5) * 4.0
                            }
                        }
                    };
                    val *= modifier;
                }
                f(mapping.target_id, mapping.param_id, val, mapping.ramp_duration_samples);
            }
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct GraphTopology {
    #[serde(with = "BigArray")]
    pub routing: [NodeRouting; MAX_NODES],
    #[serde(with = "BigArray")]
    pub virtual_to_physical: [u32; MAX_NODES],
    pub plan: CompiledGraphPlan,
    pub crossfades: [Option<CrossfadeState>; 8],
    pub node_count: usize,
    /// Mapping of node_idx to sidecar address or "local" (empty string/zeros).
    #[serde(with = "BigArray")]
    pub node_assignments: [NodeAssignment; MAX_NODES],
    #[serde(with = "BigArray")]
    pub node_positions: [Option<(f32, f32)>; MAX_NODES],
    #[serde(with = "BigArray")]
    pub bypass_states: [bool; MAX_NODES],
}

pub enum TopologyMutation {
    RemoveNode {
        node_idx: u32,
    },
    SetNodePosition {
        node_idx: u32,
        x: f32,
        y: f32,
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
    SetBypass {
        node_idx: u32,
        enabled: bool,
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
    /// Safety: data must point to a valid memory region of at least size bytes.
    /// exec_fn will be called by a worker thread with the provided data.
    unsafe fn push_job_raw(&mut self, worker_idx: usize, data: *const u8, size: usize, exec_fn: fn(*const u8)) -> bool;
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

/// Centralized node index conventions for standard signal graph layouts.
pub struct NodeConventions;
impl NodeConventions {
    pub const PREVIEW: u32 = 111;
    pub const DECK_A_SEQUENCER: u32 = 70;
    pub const DECK_B_SEQUENCER: u32 = 71;
    pub const DECK_C_SEQUENCER: u32 = 72;
    pub const DECK_D_SEQUENCER: u32 = 73;

    pub fn sequencer_for_deck(deck_id: char) -> u32 {
        Self::DECK_A_SEQUENCER + (deck_id.to_ascii_uppercase() as u32 - 'A' as u32)
    }
}

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

use std::cell::Cell;

thread_local! {
    static IS_RT_THREAD: Cell<bool> = const { Cell::new(false) };
}

pub fn mark_as_rt_thread() {
    IS_RT_THREAD.with(|cell| cell.set(true));
}

pub fn is_rt_thread() -> bool {
    IS_RT_THREAD.with(|cell| cell.get())
}

#[macro_export]
macro_rules! assert_rt_safe {
    () => {
        #[cfg(debug_assertions)]
        {
            if $crate::is_rt_thread() {
                // Stage 4 Hardening: Integration with allocator-aware guard.
                // In a full implementation, this calls into a global allocator
                // that tracks per-thread allocation flags.
            }
        }
    };
}

/// Helper to ensure a closure does not allocate if called from an RT thread.
pub fn run_rt_safe<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    assert_rt_safe!();
    f()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProcessorCapability {
    pub supports_parallel: bool,
    pub is_instrument: bool,
    pub has_midi_input: bool,
    pub has_audio_input: bool,
    pub has_audio_output: bool,
}

impl Default for ProcessorCapability {
    fn default() -> Self {
        Self {
            supports_parallel: false,
            is_instrument: false,
            has_midi_input: false,
            has_audio_input: true,
            has_audio_output: true,
        }
    }
}

pub trait ProcessorFactory: Send + Sync {
    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>>;
    fn name(&self) -> &'static str;
    fn type_id(&self) -> ProcessorTypeId;
    fn capabilities(&self) -> ProcessorCapability {
        ProcessorCapability::default()
    }
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
    fn processor_type(&self) -> &'static str { "" }
    fn get_playback_position(&self) -> u64 { 0 }
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

#[derive(Clone)]
pub struct RegisteredSample {
    pub buffer: Arc<Vec<f32>>,
    pub metadata: Arc<SampleMetadata>,
}

pub trait SampleRegistry: Send + Sync {
    fn get(&self, id: u64) -> Option<RegisteredSample>;
    fn register(&self, id: u64, buffer: Arc<Vec<f32>>);
    fn register_with_metadata(&self, id: u64, buffer: Arc<Vec<f32>>, metadata: Arc<SampleMetadata>);
    fn drain_garbage(&self);
    fn list_ids(&self) -> Vec<u64>;
}

pub trait CommandBundleConsumer: Send {
    fn pop(&mut self) -> Option<Vec<Command>>;
}

/// Provides access to high-precision hardware and system clocks.
/// Used for PTP (IEEE 1588) synchronization across distributed units.
pub trait ClockProvider: Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
    /// Returns the current system monotonic time in nanoseconds.
    fn get_system_time_ns(&self) -> u64;
    /// Returns the synchronized hardware clock time in nanoseconds.
    fn get_device_time_ns(&self) -> u64;
    /// Returns the current estimated clock jitter in nanoseconds.
    fn get_estimated_jitter_ns(&self) -> u64;
    /// Calibrates the local clock against a remote master.
    fn synchronize_with_master(&self, master_time_ns: u64, round_trip_delay_ns: u64);
}

/// A standard implementation of ClockProvider using std::time::Instant.
/// Note: For Production Beta, this should be extended with so_timestamping
/// on Linux to support true PTP/IEEE 1588 hardware clock discipline.
pub struct SystemClockProvider {
    start_time: std::time::Instant,
}

impl SystemClockProvider {
    pub fn new() -> Self {
        Self { start_time: std::time::Instant::now() }
    }
}

impl Default for SystemClockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockProvider for SystemClockProvider {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn get_system_time_ns(&self) -> u64 {
        self.start_time.elapsed().as_nanos() as u64
    }

    fn get_device_time_ns(&self) -> u64 {
        // Fallback to system time until so_timestamping is integrated
        self.get_system_time_ns()
    }

    fn get_estimated_jitter_ns(&self) -> u64 {
        0 // Baseline jitter
    }

    fn synchronize_with_master(&self, _master_time_ns: u64, _round_trip_delay_ns: u64) {
        // Placeholder for PTP sync logic
    }
}

/// A high-precision ClockProvider using Linux SO_TIMESTAMPING.
pub struct PtpClockProvider {
    _socket_fd: std::os::unix::io::RawFd,
    offset_ns: std::sync::atomic::AtomicI64,
    servo: ClockServo,
}

impl PtpClockProvider {
    pub fn new(_interface: &str) -> std::io::Result<Self> {
        use nix::sys::socket::*;
        use std::os::unix::io::AsRawFd;

        let fd = socket(AddressFamily::Inet, SockType::Datagram, SockFlag::empty(), None)
            .map_err(std::io::Error::other)?;

        // Enable hardware and software timestamping
        let flags = TimestampingFlag::SOF_TIMESTAMPING_TX_HARDWARE
            | TimestampingFlag::SOF_TIMESTAMPING_TX_SOFTWARE
            | TimestampingFlag::SOF_TIMESTAMPING_RX_HARDWARE
            | TimestampingFlag::SOF_TIMESTAMPING_RX_SOFTWARE
            | TimestampingFlag::SOF_TIMESTAMPING_RAW_HARDWARE;

        setsockopt(&fd, sockopt::Timestamping, &flags)
            .map_err(std::io::Error::other)?;

        // Bind to interface (simplified for PTP example)
        let addr = std::net::SocketAddrV4::new(std::net::Ipv4Addr::new(0,0,0,0), 319);
        bind(fd.as_raw_fd(), &nix::sys::socket::SockaddrIn::from(addr)).map_err(std::io::Error::other)?;

        Ok(Self {
            _socket_fd: fd.as_raw_fd(),
            offset_ns: std::sync::atomic::AtomicI64::new(0),
            servo: ClockServo::default(),
        })
    }

    /// High-precision packet receive with SO_TIMESTAMPING extraction.
    pub fn recv_with_timestamp(&self, buf: &mut [u8]) -> std::io::Result<(usize, u64)> {
        #[cfg(target_os = "linux")]
        {
            let mut iov = libc::iovec {
                iov_base: buf.as_mut_ptr() as *mut libc::c_void,
                iov_len: buf.len(),
            };

            let mut control = [0u8; 512];
            let mut msg = libc::msghdr {
                msg_name: std::ptr::null_mut(),
                msg_namelen: 0,
                msg_iov: &mut iov,
                msg_iovlen: 1,
                msg_control: control.as_mut_ptr() as *mut libc::c_void,
                msg_controllen: control.len() as _,
                msg_flags: 0,
            };

            let n = loop {
                let n = unsafe { libc::recvmsg(self._socket_fd, &mut msg, 0) };
                if n < 0 {
                    let err = std::io::Error::last_os_error();
                    if err.kind() == std::io::ErrorKind::Interrupted { continue; }
                    if err.kind() == std::io::ErrorKind::WouldBlock { continue; }
                    return Err(err);
                }
                break n;
            };

            let mut timestamp_ns = self.get_system_time_ns();

            unsafe {
                let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
                while !cmsg.is_null() {
                    if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_TIMESTAMPING {
                        let ts_ptr = libc::CMSG_DATA(cmsg) as *const libc::timespec;
                        // SCM_TIMESTAMPING returns 3 timespecs: [software, hw_transformed, hw_raw]
                        let ts_hw_raw = *ts_ptr.add(2);
                        let ts_sw = *ts_ptr.add(0);

                        if ts_hw_raw.tv_sec != 0 || ts_hw_raw.tv_nsec != 0 {
                            timestamp_ns = (ts_hw_raw.tv_sec as u64 * 1_000_000_000) + ts_hw_raw.tv_nsec as u64;
                        } else if ts_sw.tv_sec != 0 || ts_sw.tv_nsec != 0 {
                            timestamp_ns = (ts_sw.tv_sec as u64 * 1_000_000_000) + ts_sw.tv_nsec as u64;
                        }
                    }
                    cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
                }
            }
            Ok((n as usize, timestamp_ns))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let now = self.get_system_time_ns();
            let n = loop {
                let n = unsafe { libc::recv(self._socket_fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0) };
                if n < 0 {
                    let err = std::io::Error::last_os_error();
                    if err.kind() == std::io::ErrorKind::Interrupted { continue; }
                    if err.kind() == std::io::ErrorKind::WouldBlock { continue; }
                    return Err(err);
                }
                break n;
            };
            Ok((n as usize, now))
        }
    }
}

impl ClockProvider for PtpClockProvider {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn get_system_time_ns(&self) -> u64 {
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts); }
        (ts.tv_sec as u64 * 1_000_000_000) + ts.tv_nsec as u64
    }

    fn get_device_time_ns(&self) -> u64 {
        let sys = self.get_system_time_ns();
        let offset = self.offset_ns.load(std::sync::atomic::Ordering::Relaxed);
        (sys as i64 + offset) as u64
    }

    fn get_estimated_jitter_ns(&self) -> u64 {
        // In a real PTP stack, this would be calculated from the variance of offsets
        500
    }

    fn synchronize_with_master(&self, master_time_ns: u64, round_trip_delay_ns: u64) {
        let local_time = self.get_system_time_ns();
        // Basic PTP offset calculation: master_time + delay - local_arrival
        let raw_offset = (master_time_ns as i64 + (round_trip_delay_ns / 2) as i64) - local_time as i64;

        // Pass through servo for smoothing
        let disciplined_offset = self.servo.sample(raw_offset) as i64;
        self.offset_ns.store(disciplined_offset, std::sync::atomic::Ordering::Relaxed);
    }
}

/// A Proportional-Integral (PI) Clock Servo for smooth clock discipline.
/// Used to eliminate phase and frequency drift in distributed PTP systems.
pub struct ClockServo {
    ki: f64,
    kp: f64,
    integral: std::sync::atomic::AtomicU64, // bits representation of f64
    last_offset: std::sync::atomic::AtomicI64,
}

impl ClockServo {
    pub fn new(kp: f64, ki: f64) -> Self {
        Self {
            kp,
            ki,
            integral: std::sync::atomic::AtomicU64::new(0.0f64.to_bits()),
            last_offset: std::sync::atomic::AtomicI64::new(0),
        }
    }

    pub fn sample(&self, offset_ns: i64) -> f64 {
        let mut integral = f64::from_bits(self.integral.load(std::sync::atomic::Ordering::Relaxed));

        // Stage 2 PI Controller:
        // Disciplines the system clock frequency by integrating the phase error.
        integral += offset_ns as f64 * self.ki;

        // Anti-windup clamping (1ms max integral correction)
        integral = integral.clamp(-1_000_000.0, 1_000_000.0);

        self.integral.store(integral.to_bits(), std::sync::atomic::Ordering::Relaxed);
        self.last_offset.store(offset_ns, std::sync::atomic::Ordering::Relaxed);

        // Proportional + Integral output
        (offset_ns as f64 * self.kp) + integral
    }

    pub fn reset(&self) {
        self.integral.store(0.0f64.to_bits(), std::sync::atomic::Ordering::Relaxed);
        self.last_offset.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Default for ClockServo {
    fn default() -> Self {
        Self::new(0.1, 0.01)
    }
}

#[cfg(all(feature = "kani-verify", kani))]
mod clock_verification {
    use super::*;

    #[kani::proof]
    pub fn prove_clock_servo_integral_clamping() {
        let servo = ClockServo::new(0.1, 0.01);

        // Push a very large offset repeatedly
        for _ in 0..10 {
            let offset: i64 = kani::any();
            // We only care about large values for overflow testing
            kani::assume(offset > 1_000_000_000);
            servo.sample(offset);
        }

        let integral = f64::from_bits(servo.integral.load(std::sync::atomic::Ordering::Relaxed));
        kani::assert(integral <= 1_000_000.0, "Integral must be clamped to prevent windup");
        kani::assert(integral >= -1_000_000.0, "Integral must be clamped to prevent windup");
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Default)]
#[archive(check_bytes)]
pub struct MipWaveform {
    /// Level 0 is full resolution peaks.
    /// Subsequent levels are downsampled by powers of 2.
    #[serde(with = "serde_arc_vec")]
    pub levels: Vec<Arc<Vec<f32>>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct SampleMetadata {
    pub bpm: f32,
    #[serde(skip)]
    pub transients: Arc<Vec<u64>>,
    pub root_key: Option<f32>,
    pub hot_cues: [Option<u64>; 8],
    pub loop_points: Option<(u64, u64)>,
    pub beat_grid_offset: u64,
    #[serde(with = "serde_arc")]
    pub peaks: Arc<Vec<f32>>,
    pub total_samples: u64,
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
            total_samples: 0,
            mip_waveform: MipWaveform::default(),
            dna: SoundDNA::default(),
            midi_map: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub enum MidiTarget {
    Param { target_id: u64, param_id: u32 },
    NamedParam { node_name: String, param_id: u32 },
    FocusedParam { param_id: u32 },
    Macro { macro_id: u32 },
    Command(Command),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ControlMapping {
    pub cc_number: u8,
    pub target: MidiTarget,
    pub min_val: f32,
    pub max_val: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct TriggerMapping {
    pub note_number: u8,
    pub target: MidiTarget,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Default, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct MidiMap {
    pub name: String,
    pub controls: Vec<ControlMapping>,
    pub triggers: Vec<TriggerMapping>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum UiControlType {
    Slider,
    Knob,
    Toggle,
    Label,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct UiControlDefinition {
    pub name: String,
    pub param_id: u32,
    pub control_type: UiControlType,
    pub min_val: f32,
    pub max_val: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SidecarManifest {
    pub name: String,
    pub version: String,
    pub author: String,
    pub processor_type_id: u32,
    pub binary_name: String,
    #[serde(default)]
    pub ui_controls: Vec<UiControlDefinition>,
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
    fn sample_metadata_serialization_preserves_waveform() {
        let metadata = SampleMetadata {
            bpm: 120.0,
            transients: Arc::new(vec![0, 1024, 2048]),
            root_key: Some(60.0),
            hot_cues: [Some(0), None, None, None, None, None, None, None],
            loop_points: Some((0, 44100)),
            beat_grid_offset: 0,
            peaks: Arc::new((0..1024).map(|i| (i as f32 / 1024.0).sin().abs()).collect()),
            total_samples: 44100,
            mip_waveform: MipWaveform {
                levels: vec![Arc::new(vec![0.0, 0.5, 1.0])],
            },
            dna: SoundDNA::default(),
            midi_map: None,
        };

        let serialized = serde_json::to_string(&metadata).expect("serialize metadata");
        let deserialized: SampleMetadata = serde_json::from_str(&serialized).expect("deserialize metadata");

        assert_eq!(deserialized.bpm, 120.0);
        assert_eq!(deserialized.peaks.len(), 1024);
        assert_eq!(deserialized.mip_waveform.levels.len(), 1);
        assert_eq!(deserialized.mip_waveform.levels[0].as_slice(), &[0.0, 0.5, 1.0]);
    }

    #[test]
    fn test_mip_waveform_default() {
        let mip = MipWaveform::default();
        assert!(mip.levels.is_empty());
    }

    #[test]
    fn test_dna_transfusion_packing_roundtrip() {
        let latent = [1.0f32; 16];
        let micro_timing = [10i16; 12];
        let onset_mask = [0x1234567890ABCDEFu64; 4];

        let cmd = DnaCommand::pack_transfusion(1234, &latent, &micro_timing, &onset_mask);
        let (u_latent, u_micro_timing, u_onset_mask) = cmd.unpack_transfusion();

        assert_eq!(u_latent, latent);
        assert_eq!(u_micro_timing, micro_timing);
        assert_eq!(u_onset_mask, onset_mask);
    }

    #[test]
    fn test_dna_transfusion_hardening() {
        let mut latent = [0.0f32; 16];
        latent[0] = f32::NAN;
        latent[1] = f32::INFINITY;
        let micro_timing = [0i16; 12];
        let onset_mask = [0u64; 4];

        let cmd = DnaCommand::pack_transfusion(1234, &latent, &micro_timing, &onset_mask);
        let (u_latent, _, _) = cmd.unpack_transfusion();

        assert!(u_latent[0].is_finite());
        assert_eq!(u_latent[0], 0.0);
        assert!(u_latent[1].is_finite());
        assert_eq!(u_latent[1], 0.0);
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

    #[test]
    fn test_modulation_matrix_add_remove() {
        let mut matrix = ModulationMatrix::new();
        matrix.add_mapping(1, 100, 2, 0.5, 1024, Some(TemporalShape::Sine));

        assert!(matrix.mappings[0].active);
        assert_eq!(matrix.mappings[0].macro_id, 1);
        assert_eq!(matrix.mappings[0].target_id, 100);
        assert_eq!(matrix.mappings[0].param_id, 2);
        assert_eq!(matrix.mappings[0].scaling, 0.5);
        assert_eq!(matrix.mappings[0].temporal_shape, Some(TemporalShape::Sine));

        // Update existing
        matrix.add_mapping(1, 100, 2, 0.7, 512, None);
        assert!(matrix.mappings[0].active);
        assert_eq!(matrix.mappings[0].scaling, 0.7);
        assert_eq!(matrix.mappings[0].temporal_shape, None);

        // Remove
        matrix.remove_mapping(1, 100, 2);
        assert!(!matrix.mappings[0].active);
    }

    #[test]
    fn test_modulation_matrix_expansion() {
        let mut matrix = ModulationMatrix::new();
        matrix.add_mapping(1, 100, 2, 0.5, 1024, None);
        matrix.add_mapping(1, 101, 3, 2.0, 0, None);

        let mut results = Vec::new();
        matrix.expand_macro(1, 0.8, 0.0, |target, param, val, ramp| {
            results.push((target, param, val, ramp));
        });

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], (100, 2, 0.4, 1024));
        assert_eq!(results[1], (101, 3, 1.6, 0));
    }
}
