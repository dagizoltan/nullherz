use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BandEnergy {
    pub sub_low: f32,  // 20Hz - 100Hz
    pub low_mid: f32,  // 100Hz - 500Hz
    pub mid_high: f32, // 500Hz - 4000Hz
    pub high: f32,     // 4000Hz+
}

impl Default for BandEnergy {
    fn default() -> Self {
        Self {
            sub_low: 0.0,
            low_mid: 0.0,
            mid_high: 0.0,
            high: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnsetCandidate {
    pub frame: u64,
    pub time_sec: f64,
    pub strength: f32,
    pub band_energy: BandEnergy,
    pub spectral_flux: f32,
    pub low_freq_flux: f32,
    pub high_freq_flux: f32,
    pub complex_diff: f32,
    pub phase_deviation: f32,
    pub confidence: f32,
    pub is_ghost: bool,
    pub is_subdivision: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TempoHypothesis {
    pub bpm: f32,
    pub confidence: f32,
    pub phase_offset_frames: u64,
    pub score: f32,
}

impl Default for TempoHypothesis {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            confidence: 0.0,
            phase_offset_frames: 0,
            score: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeatMarker {
    pub frame: u64,
    pub time_sec: f64,
    pub beat_number: u32, // 0..3 (1-based musical beats 1,2,3,4)
    pub bar_number: u32,
    pub is_downbeat: bool,
    pub confidence: f32,
    pub micro_offset_ms: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PreBeatInfo {
    pub offset_ms: f32, // e.g. -75.0 ms relative to beat
    pub offset_frames: i64,
    pub recurring_confidence: f32,
    pub avg_strength: f32,
    pub primary_band: u8, // 0=sub/low, 1=mid, 2=high
}

impl Default for PreBeatInfo {
    fn default() -> Self {
        Self {
            offset_ms: 0.0,
            offset_frames: 0,
            recurring_confidence: 0.0,
            avg_strength: 0.0,
            primary_band: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrooveProfile {
    pub offsets_ms: [f32; 16],
    pub swing_ratio: f32, // 0.50 = straight, 0.66 = 2:1 swing
    pub groove_confidence: f32,
}

impl Default for GrooveProfile {
    fn default() -> Self {
        Self {
            offsets_ms: [0.0; 16],
            swing_ratio: 0.50,
            groove_confidence: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RhythmConfidence {
    pub tempo_confidence: f32,
    pub phase_confidence: f32,
    pub beat_confidence: f32,
    pub downbeat_confidence: f32,
    pub pre_beat_confidence: f32,
    pub groove_confidence: f32,
    pub grid_confidence: f32,
}

impl Default for RhythmConfidence {
    fn default() -> Self {
        Self {
            tempo_confidence: 0.0,
            phase_confidence: 0.0,
            beat_confidence: 0.0,
            downbeat_confidence: 0.0,
            pre_beat_confidence: 0.0,
            groove_confidence: 0.0,
            grid_confidence: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeatGrid {
    pub primary_bpm: f32,
    pub tempo_hypotheses: Vec<TempoHypothesis>,
    pub grid_offset_frames: u64,
    pub beats: Vec<BeatMarker>,
    pub pre_beats: Vec<PreBeatInfo>,
    pub groove: GrooveProfile,
    pub tempo_drift_bpm_per_sec: f32,
    pub confidence: RhythmConfidence,
}

impl Default for BeatGrid {
    fn default() -> Self {
        Self {
            primary_bpm: 120.0,
            tempo_hypotheses: Vec::new(),
            grid_offset_frames: 0,
            beats: Vec::new(),
            pre_beats: Vec::new(),
            groove: GrooveProfile::default(),
            tempo_drift_bpm_per_sec: 0.0,
            confidence: RhythmConfidence::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PredictiveTrackerState {
    pub current_bpm: f32,
    pub current_phase: f64, // 0.0 to 1.0 within beat
    pub last_beat_frame: u64,
    pub next_predicted_beat_frame: u64,
    pub next_predicted_pre_beat_frame: Option<u64>,
    pub tempo_drift: f32,
    pub tracking_confidence: f32,
    pub alternative_hypotheses: Vec<TempoHypothesis>,
}

impl Default for PredictiveTrackerState {
    fn default() -> Self {
        Self {
            current_bpm: 120.0,
            current_phase: 0.0,
            last_beat_frame: 0,
            next_predicted_beat_frame: 0,
            next_predicted_pre_beat_frame: None,
            tempo_drift: 0.0,
            tracking_confidence: 0.0,
            alternative_hypotheses: Vec::new(),
        }
    }
}
