use audio_dsp::rhythm::{
    BeatGridInferenceEngine, MultiFeatureOnsetDetector, MultiHypothesisTempoEstimator, RealtimePredictiveBeatTracker
};

fn generate_synthetic_beat_track(
    bpm: f32,
    sample_rate: f32,
    duration_sec: f32,
    pre_beat_offset_ms: Option<f32>,
    swing_ratio: Option<f32>,
) -> Vec<f32> {
    let total_samples = (sample_rate * duration_sec) as usize;
    let mut buffer = vec![0.0f32; total_samples];

    let seconds_per_beat = 60.0 / bpm;
    let samples_per_beat = (sample_rate * seconds_per_beat) as f64;

    let mut current_sample = 0.0f64;
    let mut beat_count = 0;

    while (current_sample as usize) < total_samples {
        let center_sample = current_sample.round() as usize;

        // Generate synthetic kick / beat hit (short burst of low freq)
        let kick_dur = (sample_rate * 0.05) as usize; // 50ms
        for i in 0..kick_dur {
            if center_sample + i < total_samples {
                let t = i as f32 / sample_rate;
                let env = (1.0 - t / 0.05).max(0.0);
                buffer[center_sample + i] += (2.0 * std::f32::consts::PI * 60.0 * t).sin() * env * 0.8;
            }
        }

        // Generate pre-beat hit if specified (e.g., -50ms, -75ms, -100ms)
        if let Some(offset_ms) = pre_beat_offset_ms {
            let pre_sample_offset = (offset_ms / 1000.0 * sample_rate) as i64;
            let pre_sample = center_sample as i64 + pre_sample_offset;

            if pre_sample >= 0 && (pre_sample as usize) < total_samples {
                let pre_idx = pre_sample as usize;
                let pre_dur = (sample_rate * 0.02) as usize; // 20ms click/snare
                for i in 0..pre_dur {
                    if pre_idx + i < total_samples {
                        let t = i as f32 / sample_rate;
                        let env = (1.0 - t / 0.02).max(0.0);
                        buffer[pre_idx + i] += (2.0 * std::f32::consts::PI * 1200.0 * t).sin() * env * 0.5;
                    }
                }
            }
        }

        beat_count += 1;
        let step_multiplier = if let Some(swing) = swing_ratio {
            if beat_count % 2 == 1 {
                swing as f64 * 2.0
            } else {
                (1.0 - swing as f64) * 2.0
            }
        } else {
            1.0
        };

        current_sample += samples_per_beat * step_multiplier;
    }

    buffer
}

#[test]
fn test_tempo_detection_across_full_range() {
    let sample_rate = 44100.0;
    let test_bpms = [30.0, 60.0, 100.0, 120.0, 140.0, 160.0, 180.0, 200.0, 220.0];

    for &target_bpm in &test_bpms {
        let buffer = generate_synthetic_beat_track(target_bpm, sample_rate, 10.0, None, None);

        let mut onset_detector = MultiFeatureOnsetDetector::new(sample_rate);
        let tempo_estimator = MultiHypothesisTempoEstimator::new(sample_rate);

        let onsets = onset_detector.process_buffer(&buffer);
        let hypotheses = tempo_estimator.estimate_tempos(&onsets);

        assert!(!hypotheses.is_empty(), "Hypotheses should not be empty for {} BPM", target_bpm);

        let detected_bpm = hypotheses[0].bpm;
        let err = (detected_bpm - target_bpm).abs() / target_bpm;

        // Allow harmonic match (half/double time) or direct match
        let is_exact = err < 0.05;
        let is_half = ((detected_bpm * 2.0) - target_bpm).abs() / target_bpm < 0.05;
        let is_double = ((detected_bpm / 2.0) - target_bpm).abs() / target_bpm < 0.05;

        assert!(
            is_exact || is_half || is_double,
            "Target {} BPM detected as {} BPM (err {})",
            target_bpm,
            detected_bpm,
            err
        );
    }
}

#[test]
fn test_pre_beat_detection_does_not_shift_structural_grid() {
    let sample_rate = 44100.0;
    let target_bpm = 120.0;

    let offsets_to_test = [-50.0f32, -75.0f32, -100.0f32];

    for &pre_offset_ms in &offsets_to_test {
        let buffer = generate_synthetic_beat_track(target_bpm, sample_rate, 8.0, Some(pre_offset_ms), None);

        let mut onset_detector = MultiFeatureOnsetDetector::new(sample_rate);
        let tempo_estimator = MultiHypothesisTempoEstimator::new(sample_rate);
        let grid_engine = BeatGridInferenceEngine::new(sample_rate);

        let onsets = onset_detector.process_buffer(&buffer);
        let hypotheses = tempo_estimator.estimate_tempos(&onsets);
        let grid = grid_engine.infer_beat_grid(&onsets, &hypotheses, buffer.len() as u64);

        // Grid primary BPM should stay locked to target_bpm
        let bpm_diff = (grid.primary_bpm - target_bpm).abs();
        assert!(bpm_diff < 3.0, "Grid BPM shifted from {} to {}", target_bpm, grid.primary_bpm);

        // Grid phase offset should remain close to 0 or 1 beat period (structural beat) rather than shifted by pre-beat
        let expected_samples_per_beat = (sample_rate * 60.0 / target_bpm) as f64;
        let phase_offset = grid.grid_offset_frames as f64 % expected_samples_per_beat;
        let min_phase_diff = phase_offset.min(expected_samples_per_beat - phase_offset);
        let phase_diff_ms = (min_phase_diff / sample_rate as f64) * 1000.0;

        assert!(
            phase_diff_ms < 25.0,
            "Grid phase shifted by {} ms due to pre-beat (pre_offset = {} ms)",
            phase_diff_ms,
            pre_offset_ms
        );

        // Pre-beat should be detected explicitly in pre_beats
        assert!(
            !grid.pre_beats.is_empty(),
            "Pre-beat failed to be detected for offset {} ms",
            pre_offset_ms
        );
    }
}

#[test]
fn test_realtime_predictive_tracker_stability() {
    let sample_rate = 44100.0;
    let target_bpm = 128.0;

    let buffer = generate_synthetic_beat_track(target_bpm, sample_rate, 10.0, None, None);

    let mut onset_detector = MultiFeatureOnsetDetector::new(sample_rate);
    let tempo_estimator = MultiHypothesisTempoEstimator::new(sample_rate);
    let grid_engine = BeatGridInferenceEngine::new(sample_rate);

    let onsets = onset_detector.process_buffer(&buffer);
    let hypotheses = tempo_estimator.estimate_tempos(&onsets);
    let grid = grid_engine.infer_beat_grid(&onsets, &hypotheses, buffer.len() as u64);

    let mut tracker = RealtimePredictiveBeatTracker::new(sample_rate);
    tracker.set_beat_grid(grid);

    // Simulate 128-sample block processing loop
    let block_size = 128u32;
    let num_blocks = buffer.len() as u32 / block_size;

    for _ in 0..num_blocks {
        tracker.update(block_size, &[]);
    }

    assert!((tracker.state().current_bpm - target_bpm).abs() < 2.0);
}
