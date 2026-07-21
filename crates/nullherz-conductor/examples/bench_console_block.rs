//! Per-block latency benchmark for the bootstrapped 4-deck console — the
//! measurement fixture the 2026-07-21 hot-path audit called for.
//!
//! Drives `process_block` exactly like the offline bounce / golden-render
//! path (no backend thread), so the numbers isolate the engine itself:
//! graph scheduling, worker-pool dispatch, DSP, telemetry finalization.
//! The worker pool IS active (engine default), so pool overhead is included.
//! All four decks play multitone stereo material through the full strip
//! chain into the mastered stereo sum.
//!
//! Run:
//!   cargo run --release -p nullherz-conductor --example bench_console_block
//!
//! Compare runs on the same machine only; the absolute numbers depend on
//! core count, governor, and whether RT scheduling was granted to workers.

use std::sync::Arc;
use std::time::Instant;

use nullherz_conductor::Conductor;
use nullherz_dna::GeneticLibrary;
use nullherz_traits::{Command, PerformanceCommand};

const SR: f32 = 44_100.0;
const BLOCK: usize = 256;
/// Same deterministic pre-roll the golden render uses to stream-install the
/// bootstrap topology (bounded mutations per block).
const INSTALL_BLOCKS: usize = 256;
const ARM_BLOCKS: usize = 8;
const WARMUP_BLOCKS: usize = 2_000;
/// Default measurement length; override with NULLHERZ_BENCH_BLOCKS for
/// shorter interleaved A/B runs (thermal drift on laptop-class hardware
/// swamps cross-run comparisons of long back-to-back runs).
const MEASURE_BLOCKS: usize = 20_000;

fn measure_blocks() -> usize {
    std::env::var("NULLHERZ_BENCH_BLOCKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(MEASURE_BLOCKS)
}

const PARTIAL_AMPS: [f32; 3] = [0.275, 0.15, 0.075];

/// Long enough to outlast install + arm + warmup + measurement
/// ((2000 + 20000 + 264) blocks ≈ 129 s) — a SamplerVoice deactivates at
/// buffer end, and a dead voice would make the "DSP" half of the benchmark
/// silence.
const TONE_SECONDS: f32 = 135.0;

fn register_stereo_tone(conductor: &Conductor, id: u64, left_hz: [f32; 3], right_hz: [f32; 3]) {
    let frames = (SR * TONE_SECONDS) as usize;
    let mut samples = Vec::with_capacity(frames * 2);
    for set in [left_hz, right_hz] {
        for i in 0..frames {
            let mut s = 0.0f32;
            for (hz, amp) in set.iter().zip(PARTIAL_AMPS) {
                s += (i as f32 * hz * 2.0 * std::f32::consts::PI / SR).sin() * amp;
            }
            samples.push(s);
        }
    }

    let mut metadata = nullherz_traits::SampleMetadata::new_empty();
    metadata.bpm = 120.0;
    metadata.total_samples = frames as u64;
    metadata.channels = 2;
    let metadata = Arc::new(metadata);
    conductor
        .transfusion_manager
        .sample_registry
        .register_with_metadata(id, Arc::new(samples), metadata.clone());

    // The deck-load path resolves the track through the library row.
    let lib = conductor.library.lock();
    lib.save_track(&nullherz_dna::LibraryTrack {
        id,
        path: format!("tone://{}", id),
        title: "bench stereo tone".to_string(),
        artist: "bench".to_string(),
        album: "bench".to_string(),
        genre: "test tone".to_string(),
        energy_level: 0.5,
        metadata,
    })
    .expect("in-memory library save cannot fail");
}

fn pump_block(conductor: &mut Conductor, left: &mut [f32], right: &mut [f32]) {
    let inputs: Vec<&[f32]> = vec![];
    let mut outputs = vec![left, right];
    let mut engine_lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
    let engine_arc = engine_lock.as_mut().expect("setup_engine must install an engine");
    if let Some(engine) = Arc::get_mut(engine_arc) {
        engine.process_block(&inputs, &mut outputs, BLOCK);
    } else {
        // No other thread runs in this harness; exclusive access holds by
        // construction (same pattern as the golden render test).
        let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn nullherz_traits::RenderingEngine;
        unsafe { (*engine_ptr).process_block(&inputs, &mut outputs, BLOCK); }
    }
}

fn main() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    register_stereo_tone(&conductor, 9_901, [220.0, 1_470.0, 6_300.0], [330.0, 2_210.0, 9_500.0]);
    register_stereo_tone(&conductor, 9_902, [440.0, 3_150.0, 11_000.0], [550.0, 4_400.0, 13_200.0]);

    let mut left = vec![0.0f32; BLOCK];
    let mut right = vec![0.0f32; BLOCK];

    for _ in 0..INSTALL_BLOCKS {
        pump_block(&mut conductor, &mut left, &mut right);
    }

    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: 9_901 }),
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'B', sample_id: 9_902 }),
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'C', sample_id: 9_901 }),
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'D', sample_id: 9_902 }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'B' }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'C' }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'D' }),
    ]);
    for _ in 0..ARM_BLOCKS {
        pump_block(&mut conductor, &mut left, &mut right);
    }

    for _ in 0..WARMUP_BLOCKS {
        pump_block(&mut conductor, &mut left, &mut right);
    }

    // Keep the render audibly alive across the measurement (looping decks);
    // sanity-check output is hot so a silent misconfiguration can't
    // masquerade as a fast run.
    let mut peak = 0.0f32;

    let measure = measure_blocks();
    let mut samples_ns: Vec<u64> = Vec::with_capacity(measure);
    for _ in 0..measure {
        let t0 = Instant::now();
        pump_block(&mut conductor, &mut left, &mut right);
        samples_ns.push(t0.elapsed().as_nanos() as u64);
        for i in 0..BLOCK {
            peak = peak.max(left[i].abs()).max(right[i].abs());
        }
    }

    samples_ns.sort_unstable();
    let sum: u64 = samples_ns.iter().sum();
    let mean = sum as f64 / samples_ns.len() as f64;
    let pct = |p: f64| samples_ns[((samples_ns.len() as f64 * p) as usize).min(samples_ns.len() - 1)];
    let budget_ns = BLOCK as f64 / SR as f64 * 1e9;

    println!("blocks measured : {}", measure);
    println!("block size      : {} @ {} Hz (budget {:.0} us)", BLOCK, SR, budget_ns / 1e3);
    println!("output peak     : {:.4} (must be > 0 — silence means a broken run)", peak);
    println!("mean            : {:>9.2} us  ({:.1}% of budget)", mean / 1e3, mean / budget_ns * 100.0);
    println!("p50             : {:>9.2} us", pct(0.50) as f64 / 1e3);
    println!("p90             : {:>9.2} us", pct(0.90) as f64 / 1e3);
    println!("p99             : {:>9.2} us", pct(0.99) as f64 / 1e3);
    println!("p99.9           : {:>9.2} us", pct(0.999) as f64 / 1e3);
    println!("max             : {:>9.2} us", pct(1.0) as f64 / 1e3);

    assert!(peak > 0.0, "benchmark rendered silence — decks did not play");
}
