use crate::*;

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
    pub const MASTERING_EQ: Self = Self(220);
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
