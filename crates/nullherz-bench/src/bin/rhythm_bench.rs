use audio_dsp::rhythm::{
    BeatGridInferenceEngine, MultiFeatureOnsetDetector, MultiHypothesisTempoEstimator, RealtimePredictiveBeatTracker
};
use std::time::Instant;

fn main() {
    println!("=== Rhythm & Beat-Tracking Benchmarks ===");

    let sample_rate = 44100.0;
    let duration_sec = 60.0; // 1 minute synthetic track
    let total_samples = (sample_rate * duration_sec) as usize;
    let mut buffer = vec![0.0f32; total_samples];

    // Populating 128 BPM track with kicks every 0.46875 seconds
    let spb = (sample_rate * 60.0 / 128.0) as usize;
    for i in (0..total_samples).step_by(spb) {
        for k in 0..200 {
            if i + k < total_samples {
                buffer[i + k] = 0.8;
            }
        }
    }

    // 1. Onset Detection Benchmark
    let mut onset_detector = MultiFeatureOnsetDetector::new(sample_rate);
    let start_onset = Instant::now();
    let onsets = onset_detector.process_buffer(&buffer);
    let dur_onset = start_onset.elapsed();
    println!("1-minute track Onset Detection: {:?} (found {} onsets)", dur_onset, onsets.len());

    // 2. Multi-Hypothesis Tempo Estimation Benchmark
    let tempo_estimator = MultiHypothesisTempoEstimator::new(sample_rate);
    let start_tempo = Instant::now();
    let hypotheses = tempo_estimator.estimate_tempos(&onsets);
    let dur_tempo = start_tempo.elapsed();
    println!("Tempo Estimation: {:?} (primary BPM: {:.2})", dur_tempo, hypotheses[0].bpm);

    // 3. Beat-Grid Inference Benchmark
    let grid_engine = BeatGridInferenceEngine::new(sample_rate);
    let start_grid = Instant::now();
    let grid = grid_engine.infer_beat_grid(&onsets, &hypotheses, total_samples as u64);
    let dur_grid = start_grid.elapsed();
    println!("Beat Grid Inference: {:?} (total beats: {})", dur_grid, grid.beats.len());

    // 4. Real-time Predictive Tracker Latency Benchmark
    let mut tracker = RealtimePredictiveBeatTracker::new(sample_rate);
    tracker.set_beat_grid(grid);

    let start_rt = Instant::now();
    let block_size = 128u32;
    let iterations = 10_000;
    for _ in 0..iterations {
        tracker.update(block_size, &[]);
    }
    let dur_rt = start_rt.elapsed();
    println!(
        "Real-time Predictive Tracker 10,000 blocks ({:.2}s of audio): {:?} ({:.3} ns per block)",
        (iterations * block_size) as f32 / sample_rate,
        dur_rt,
        dur_rt.as_nanos() as f64 / iterations as f64
    );

    println!("=== Benchmark Complete ===");
}
