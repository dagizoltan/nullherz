use std::sync::Arc;
use crate::*;

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
    #[serde(with = "crate::serde_helpers::serde_arc_vec")]
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
    #[serde(with = "crate::serde_helpers::serde_arc")]
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
