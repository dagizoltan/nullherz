use super::types::{BeatGrid, OnsetCandidate, PredictiveTrackerState};

pub struct RealtimePredictiveBeatTracker {
    sample_rate: f32,
    state: PredictiveTrackerState,
    accumulated_frames: u64,
    grid: Option<BeatGrid>,
}

impl RealtimePredictiveBeatTracker {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            state: PredictiveTrackerState::default(),
            accumulated_frames: 0,
            grid: None,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    pub fn set_beat_grid(&mut self, grid: BeatGrid) {
        self.state.current_bpm = grid.primary_bpm;
        self.state.tracking_confidence = grid.confidence.grid_confidence;
        self.state.alternative_hypotheses = grid.tempo_hypotheses.clone();

        self.state.next_predicted_beat_frame = grid.grid_offset_frames;

        if let Some(pre) = grid.pre_beats.first() {
            let pre_offset = pre.offset_frames;
            if (grid.grid_offset_frames as i64 + pre_offset) >= 0 {
                self.state.next_predicted_pre_beat_frame =
                    Some((grid.grid_offset_frames as i64 + pre_offset) as u64);
            }
        }

        self.grid = Some(grid);
    }

    pub fn state(&self) -> &PredictiveTrackerState {
        &self.state
    }

    /// Real-time safe state update called on every audio block or streaming onset buffer.
    /// Zero heap allocations in this execution path.
    pub fn update(&mut self, block_frames: u32, live_onsets: &[OnsetCandidate]) {
        self.accumulated_frames += block_frames as u64;

        let current_bpm = self.state.current_bpm.max(30.0);
        let samples_per_beat = (self.sample_rate as f64 * 60.0 / current_bpm as f64).max(1.0);

        // Advance continuous phase
        let phase_advance = block_frames as f64 / samples_per_beat;
        self.state.current_phase = (self.state.current_phase + phase_advance).fract();

        // Check if we passed the predicted next beat
        if self.accumulated_frames >= self.state.next_predicted_beat_frame {
            self.state.last_beat_frame = self.state.next_predicted_beat_frame;
            self.state.next_predicted_beat_frame =
                (self.state.last_beat_frame as f64 + samples_per_beat).round() as u64;

            // Recalculate pre-beat prediction if pre-beat model exists
            if let Some(ref g) = self.grid {
                if let Some(pre) = g.pre_beats.first() {
                    let next_pre = self.state.next_predicted_beat_frame as i64 + pre.offset_frames;
                    if next_pre >= 0 {
                        self.state.next_predicted_pre_beat_frame = Some(next_pre as u64);
                    }
                }
            }
        }

        // PLL Phase Correction: adjust tempo / phase slightly based on live onsets near predicted beat
        for onset in live_onsets {
            let dist = (onset.frame as i64 - self.state.next_predicted_beat_frame as i64).abs();
            let window = (samples_per_beat * 0.1) as i64;

            if dist < window {
                let err_frames = onset.frame as i64 - self.state.next_predicted_beat_frame as i64;
                let err_sec = err_frames as f32 / self.sample_rate;

                // Gentle proportional correction to prevent jumping
                let alpha = 0.0005f32;
                self.state.tempo_drift += err_sec * alpha;
                self.state.current_bpm = (self.state.current_bpm - self.state.tempo_drift).clamp(30.0, 220.0);
            }
        }
    }
}
