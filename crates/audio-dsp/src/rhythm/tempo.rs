use super::types::{OnsetCandidate, TempoHypothesis};
use std::collections::HashMap;

pub struct MultiHypothesisTempoEstimator {
    min_bpm: f32,
    max_bpm: f32,
    sample_rate: f32,
}

impl MultiHypothesisTempoEstimator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            min_bpm: 30.0,
            max_bpm: 220.0,
            sample_rate,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    /// Analyze onset candidates and compute multiple competing tempo hypotheses.
    pub fn estimate_tempos(&self, onsets: &[OnsetCandidate]) -> Vec<TempoHypothesis> {
        if onsets.len() < 3 {
            return vec![
                TempoHypothesis {
                    bpm: 120.0,
                    confidence: 0.5,
                    phase_offset_frames: 0,
                    score: 0.5,
                },
                TempoHypothesis {
                    bpm: 60.0,
                    confidence: 0.2,
                    phase_offset_frames: 0,
                    score: 0.2,
                },
            ];
        }

        // Calculate inter-onset intervals (IOIs)
        let mut intervals_sec = Vec::new();
        for i in 0..onsets.len() {
            for j in (i + 1)..(i + 12).min(onsets.len()) {
                let diff_sec = onsets[j].time_sec - onsets[i].time_sec;
                let weight = onsets[i].strength * onsets[j].strength;
                if diff_sec >= (60.0 / self.max_bpm as f64) && diff_sec <= (60.0 / self.min_bpm as f64) * 4.0 {
                    intervals_sec.push((diff_sec, weight));
                }
            }
        }

        if intervals_sec.is_empty() {
            return vec![TempoHypothesis {
                bpm: 120.0,
                confidence: 0.5,
                phase_offset_frames: 0,
                score: 0.5,
            }];
        }

        // Auto-correlation / Comb filter scoring over BPM range [30.0 .. 220.0]
        let mut histogram: HashMap<u32, f32> = HashMap::new(); // key is BPM * 10
        let step_bpm = 0.5f32;
        let mut bpm_scores = Vec::new();

        let mut curr_bpm = self.min_bpm;
        while curr_bpm <= self.max_bpm {
            let period_sec = 60.0 / curr_bpm as f64;
            let mut score = 0.0f32;

            for &(ioi, weight) in &intervals_sec {
                let ratio = ioi / period_sec;
                let nearest_int = ratio.round();
                if nearest_int >= 1.0 && nearest_int <= 4.0 {
                    let diff = (ratio - nearest_int).abs();
                    if diff < 0.12 {
                        let harmonics_weight = 1.0 / (nearest_int as f32);
                        score += weight * (1.0 - diff as f32 / 0.12) * harmonics_weight;
                    }
                }
            }

            // Metrical preference weighting (slight prior for typical musical tempos 80-140 BPM)
            let metrical_prior = 1.0 + 0.15 * (-((curr_bpm - 115.0) / 45.0).powi(2)).exp();
            let final_score = score * metrical_prior;

            bpm_scores.push((curr_bpm, final_score));
            histogram.insert((curr_bpm * 10.0) as u32, final_score);

            curr_bpm += step_bpm;
        }

        // Peak picking over BPM histogram
        bpm_scores.sort_by(|a, b| b.1.total_cmp(&a.1));

        let mut hypotheses = Vec::new();
        for &(bpm, score) in &bpm_scores {
            if hypotheses.len() >= 5 {
                break;
            }
            if score <= 0.001 {
                continue;
            }

            // Ensure hypotheses are distinct (at least 5% away from already chosen ones)
            let is_duplicate = hypotheses.iter().any(|h: &TempoHypothesis| {
                (h.bpm - bpm).abs() / h.bpm < 0.04
            });

            if !is_duplicate {
                // Compute phase offset for this tempo
                let period_frames = (self.sample_rate * 60.0 / bpm) as u64;
                let phase_offset = if period_frames > 0 {
                    let mut phase_accum = 0u64;
                    let mut phase_count = 0u32;
                    for o in onsets.iter().take(20) {
                        phase_accum += o.frame % period_frames;
                        phase_count += 1;
                    }
                    if phase_count > 0 {
                        phase_accum / phase_count as u64
                    } else {
                        0
                    }
                } else {
                    0
                };

                hypotheses.push(TempoHypothesis {
                    bpm,
                    confidence: 0.0, // calculated below
                    phase_offset_frames: phase_offset,
                    score,
                });
            }
        }

        if hypotheses.is_empty() {
            return vec![TempoHypothesis {
                bpm: 120.0,
                confidence: 0.5,
                phase_offset_frames: 0,
                score: 0.5,
            }];
        }

        // Calculate relative confidence
        let total_score: f32 = hypotheses.iter().map(|h| h.score).sum::<f32>().max(1e-6);
        for h in &mut hypotheses {
            h.confidence = (h.score / total_score).clamp(0.01, 0.99);
        }

        // Half-time / double-time explicit handling:
        // Ensure half-time and double-time variants are explicitly represented if absent
        let primary_bpm = hypotheses[0].bpm;
        let half_bpm = primary_bpm / 2.0;
        let double_bpm = primary_bpm * 2.0;

        if half_bpm >= self.min_bpm && !hypotheses.iter().any(|h| (h.bpm - half_bpm).abs() < 2.0) {
            hypotheses.push(TempoHypothesis {
                bpm: half_bpm,
                confidence: hypotheses[0].confidence * 0.4,
                phase_offset_frames: hypotheses[0].phase_offset_frames,
                score: hypotheses[0].score * 0.4,
            });
        }

        if double_bpm <= self.max_bpm && !hypotheses.iter().any(|h| (h.bpm - double_bpm).abs() < 2.0) {
            hypotheses.push(TempoHypothesis {
                bpm: double_bpm,
                confidence: hypotheses[0].confidence * 0.3,
                phase_offset_frames: hypotheses[0].phase_offset_frames,
                score: hypotheses[0].score * 0.3,
            });
        }

        hypotheses
    }
}
