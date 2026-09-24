use super::types::{
    BeatGrid, BeatMarker, GrooveProfile, OnsetCandidate, PreBeatInfo, RhythmConfidence,
    TempoHypothesis,
};

pub struct BeatGridInferenceEngine {
    sample_rate: f32,
}

impl BeatGridInferenceEngine {
    pub fn new(sample_rate: f32) -> Self {
        Self { sample_rate }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    /// Infer structural beat grid, pre-beats, groove, swing, downbeats, and confidence.
    pub fn infer_beat_grid(
        &self,
        onsets: &[OnsetCandidate],
        hypotheses: &[TempoHypothesis],
        total_frames: u64,
    ) -> BeatGrid {
        if hypotheses.is_empty() {
            return BeatGrid::default();
        }

        let primary_bpm = hypotheses[0].bpm;
        let samples_per_beat = (self.sample_rate as f64 * 60.0 / primary_bpm as f64) as f64;

        if samples_per_beat <= 0.0 || total_frames == 0 {
            return BeatGrid::default();
        }

        // 1. Phase alignment via Viterbi / Dynamic Programming over onset candidates
        let mut best_phase_offset = 0.0f64;
        let mut max_evidence = -1.0f32;

        let search_steps = 100;
        for s in 0..search_steps {
            let phase_candidate = (s as f64 / search_steps as f64) * samples_per_beat;
            let mut evidence = 0.0f32;

            for o in onsets {
                let dist_from_grid = ((o.frame as f64 - phase_candidate).rem_euclid(samples_per_beat))
                    .min(samples_per_beat - (o.frame as f64 - phase_candidate).rem_euclid(samples_per_beat));
                let dist_norm = dist_from_grid / samples_per_beat;

                if dist_norm < 0.08 {
                    // Strong evidence for structural beat: weight low-frequency (kick) energy higher than mid/high pre-beats
                    let proximity_weight = 1.0 - (dist_norm / 0.08) as f32;
                    let low_freq_weight = 1.0 + (o.band_energy.sub_low + o.band_energy.low_mid) * 2.0;
                    evidence += o.strength * proximity_weight * low_freq_weight;
                }
            }

            if evidence > max_evidence {
                max_evidence = evidence;
                best_phase_offset = phase_candidate;
            }
        }

        let grid_offset_frames = best_phase_offset.round() as u64;

        // 2. Generate Structural Beat Markers
        let total_beats = ((total_frames as f64 - best_phase_offset) / samples_per_beat).floor() as usize;
        let mut beats = Vec::with_capacity(total_beats);

        for b in 0..total_beats {
            let beat_frame = (best_phase_offset + b as f64 * samples_per_beat).round() as u64;
            let beat_time = beat_frame as f64 / self.sample_rate as f64;

            // Find closest onset to measure micro-timing
            let mut closest_dev_ms = 0.0f32;
            let mut min_dist = samples_per_beat * 0.2;

            for o in onsets {
                let diff_frames = o.frame as f64 - beat_frame as f64;
                if diff_frames.abs() < min_dist {
                    min_dist = diff_frames.abs();
                    closest_dev_ms = (diff_frames / self.sample_rate as f64 * 1000.0) as f32;
                }
            }

            let beat_in_bar = (b % 4) as u32; // 0, 1, 2, 3
            let bar_number = (b / 4) as u32;
            let is_downbeat = beat_in_bar == 0;

            beats.push(BeatMarker {
                frame: beat_frame,
                time_sec: beat_time,
                beat_number: beat_in_bar,
                bar_number,
                is_downbeat,
                confidence: (hypotheses[0].confidence * 1.2).clamp(0.1, 0.99),
                micro_offset_ms: closest_dev_ms,
            });
        }

        // 3. Pre-Beat / Anticipation Detection (-120ms to -30ms before beat)
        // Explicitly models recurring pre-hits without shifting structural beat grid
        let pre_beat_info = self.detect_pre_beats(onsets, &beats, samples_per_beat);

        // 4. Groove & Swing Analysis
        let groove = self.analyze_groove_and_swing(onsets, &beats, samples_per_beat);

        // 5. Overall Confidence Calculation
        let grid_confidence = RhythmConfidence {
            tempo_confidence: hypotheses[0].confidence,
            phase_confidence: if max_evidence > 1.0 { 0.9 } else { 0.5 },
            beat_confidence: 0.85,
            downbeat_confidence: 0.80,
            pre_beat_confidence: pre_beat_info.iter().map(|p| p.recurring_confidence).fold(0.0f32, f32::max),
            groove_confidence: groove.groove_confidence,
            grid_confidence: (hypotheses[0].confidence * 0.6 + 0.4).clamp(0.0, 1.0),
        };

        BeatGrid {
            primary_bpm,
            tempo_hypotheses: hypotheses.to_vec(),
            grid_offset_frames,
            beats,
            pre_beats: pre_beat_info,
            groove,
            tempo_drift_bpm_per_sec: 0.0,
            confidence: grid_confidence,
        }
    }

    fn detect_pre_beats(
        &self,
        onsets: &[OnsetCandidate],
        beats: &[BeatMarker],
        samples_per_beat: f64,
    ) -> Vec<PreBeatInfo> {
        if beats.is_empty() || onsets.is_empty() {
            return Vec::new();
        }

        let mut pre_beat_diffs = Vec::new();

        for b in beats {
            let beat_frame = b.frame as f64;
            let window_start = beat_frame - samples_per_beat * 0.25; // up to -150ms at 120bpm
            let window_end = beat_frame - samples_per_beat * 0.05;   // at least -30ms before

            for o in onsets {
                let o_frame = o.frame as f64;
                if o_frame >= window_start && o_frame <= window_end {
                    let diff_ms = ((o_frame - beat_frame) / self.sample_rate as f64) * 1000.0;
                    pre_beat_diffs.push((diff_ms, o.strength, o.band_energy));
                }
            }
        }

        if pre_beat_diffs.len() < 3 {
            return Vec::new();
        }

        // Clustering pre-beat offsets into 10ms histogram bins
        let mut histogram: std::collections::HashMap<i32, (usize, f32)> = std::collections::HashMap::new();
        for &(diff_ms, strength, _) in &pre_beat_diffs {
            let bin = (diff_ms / 10.0).round() as i32 * 10;
            let entry = histogram.entry(bin).or_insert((0, 0.0));
            entry.0 += 1;
            entry.1 += strength;
        }

        let mut pre_beats = Vec::new();
        for (&bin_ms, &(count, total_strength)) in &histogram {
            // Require repetition across bars (at least 20% of beats or 3 occurrences)
            if count >= 3 && (count as f32 / beats.len() as f32) > 0.15 {
                let avg_strength = total_strength / count as f32;
                let recurring_confidence = (count as f32 / beats.len() as f32 * 2.0).clamp(0.4, 0.98);

                pre_beats.push(PreBeatInfo {
                    offset_ms: bin_ms as f32,
                    offset_frames: (bin_ms as f64 / 1000.0 * self.sample_rate as f64).round() as i64,
                    recurring_confidence,
                    avg_strength,
                    primary_band: 1, // mid band
                });
            }
        }

        pre_beats.sort_by(|a, b| b.recurring_confidence.total_cmp(&a.recurring_confidence));
        pre_beats
    }

    fn analyze_groove_and_swing(
        &self,
        onsets: &[OnsetCandidate],
        beats: &[BeatMarker],
        samples_per_beat: f64,
    ) -> GrooveProfile {
        if beats.len() < 2 {
            return GrooveProfile::default();
        }

        let samples_per_16th = samples_per_beat / 4.0;
        let mut buckets_ms: [Vec<f32>; 16] = Default::default();
        let mut offbeat_8th_offsets = Vec::new();

        for b in beats {
            let beat_frame = b.frame as f64;
            for o in onsets {
                let diff_frames = o.frame as f64 - beat_frame;
                if diff_frames >= 0.0 && diff_frames < samples_per_beat {
                    let step_16th = (diff_frames / samples_per_16th).round() as usize % 16;
                    let dev_ms = ((diff_frames - step_16th as f64 * samples_per_16th) / self.sample_rate as f64) * 1000.0;
                    buckets_ms[step_16th].push(dev_ms as f32);

                    // Off-beat 8th note is step 2 in a beat (or step 2, 6, 10, 14 in bar)
                    if step_16th == 2 {
                        offbeat_8th_offsets.push(diff_frames / samples_per_beat);
                    }
                }
            }
        }

        let mut offsets_ms = [0.0f32; 16];
        for i in 0..16 {
            if !buckets_ms[i].is_empty() {
                offsets_ms[i] = buckets_ms[i].iter().sum::<f32>() / buckets_ms[i].len() as f32;
            }
        }

        // Calculate Swing Ratio
        let swing_ratio = if !offbeat_8th_offsets.is_empty() {
            let avg_pos = offbeat_8th_offsets.iter().sum::<f64>() / offbeat_8th_offsets.len() as f64;
            avg_pos.clamp(0.40, 0.75) as f32
        } else {
            0.50
        };

        GrooveProfile {
            offsets_ms,
            swing_ratio,
            groove_confidence: 0.75,
        }
    }
}
