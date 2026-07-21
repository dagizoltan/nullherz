//! Per-node cost profile of the bootstrapped 4-deck console.
//!
//! Answers "where does the block time actually go?" using the engine's own
//! per-node cycle telemetry, so the next optimization targets the measured hot
//! nodes instead of a guess. Same setup and program material as
//! bench_console_block (4 decks, multitone stereo), driven through the engine
//! directly (no backend thread). Times are averaged over the measurement
//! window; the RANKING and % share are what matter (ns scaling depends on the
//! calibrated ns_per_cycle, but every node shares the same factor).
//!
//! Run:
//!   cargo run --release -p nullherz-conductor --example profile_console_nodes

use std::collections::HashMap;
use std::sync::Arc;

use nullherz_conductor::{Conductor, EngineContext};
use nullherz_dna::GeneticLibrary;
use nullherz_traits::{Command, PerformanceCommand, MAX_NODES};

const SR: f32 = 44_100.0;
const BLOCK: usize = 256;
const INSTALL_BLOCKS: usize = 256;
const ARM_BLOCKS: usize = 8;
const WARMUP_BLOCKS: usize = 2_000;
const MEASURE_BLOCKS: usize = 8_000;
const PARTIAL_AMPS: [f32; 3] = [0.275, 0.15, 0.075];
const TONE_SECONDS: f32 = 60.0;

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
    let lib = conductor.library.lock();
    lib.save_track(&nullherz_dna::LibraryTrack {
        id,
        path: format!("tone://{}", id),
        title: "profile tone".to_string(),
        artist: "profile".to_string(),
        album: "profile".to_string(),
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
        let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn nullherz_traits::RenderingEngine;
        unsafe { (*engine_ptr).process_block(&inputs, &mut outputs, BLOCK); }
    }
}

fn type_name(id: u32) -> &'static str {
    match id {
        0 => "Delay",
        1 => "Biquad",
        2 => "Gain",
        10 => "Sampler",
        11 => "BiquadEq",
        20 => "Crossfader",
        30 => "Summing",
        40 => "Spectral",
        50 => "Wavetable",
        60 => "Modulation",
        70 => "Sequencer",
        80 => "EnvFollower",
        90 => "Granular",
        100 => "SpectralMorph",
        110 => "Capture",
        120 => "DjIsolator",
        130 => "SimdBiquad",
        140 => "KeySync",
        150 => "Personality",
        190 => "DnaMorph",
        200 => "Limiter",
        210 => "StreamSampler",
        220 => "MasteringEq",
        _ => "?",
    }
}

fn main() {
    let mut conductor = Conductor::with_library_path(":memory:");
    let EngineContext { mut telemetry_consumer, .. } = conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    register_stereo_tone(&conductor, 9_901, [220.0, 1_470.0, 6_300.0], [330.0, 2_210.0, 9_500.0]);
    register_stereo_tone(&conductor, 9_902, [440.0, 3_150.0, 11_000.0], [550.0, 4_400.0, 13_200.0]);

    let mut left = vec![0.0f32; BLOCK];
    let mut right = vec![0.0f32; BLOCK];
    for _ in 0..INSTALL_BLOCKS { pump_block(&mut conductor, &mut left, &mut right); }

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
    for _ in 0..ARM_BLOCKS { pump_block(&mut conductor, &mut left, &mut right); }

    // Reverse name map (index -> name) and type map from the live console.
    let idx_to_name: HashMap<u32, String> = conductor
        .mixer_manager
        .node_names
        .iter()
        .map(|(name, &idx)| (idx, name.clone()))
        .collect();
    let idx_to_type = conductor.topology_manager.active_node_types.clone();

    for _ in 0..WARMUP_BLOCKS { pump_block(&mut conductor, &mut left, &mut right); }
    while telemetry_consumer.pop().is_some() {} // drain warmup telemetry

    // Accumulate per-node time over the measurement window.
    let mut sum_ns = [0u64; MAX_NODES];
    let mut snapshots = 0u64;
    let mut peak = 0.0f32;
    for _ in 0..MEASURE_BLOCKS {
        pump_block(&mut conductor, &mut left, &mut right);
        for i in 0..BLOCK { peak = peak.max(left[i].abs()).max(right[i].abs()); }
        while let Some(tel) = telemetry_consumer.pop() {
            for n in 0..MAX_NODES { sum_ns[n] += tel.node_times_ns[n]; }
            snapshots += 1;
        }
    }
    assert!(peak > 0.0, "profile rendered silence — decks did not play");
    let snapshots = snapshots.max(1);

    // Rank nodes by average cost.
    let mut rows: Vec<(usize, u64)> = (0..MAX_NODES)
        .map(|n| (n, sum_ns[n] / snapshots))
        .filter(|&(_, avg)| avg > 0)
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let total: u64 = rows.iter().map(|(_, v)| v).sum();

    println!("per-node cost profile — {} snapshots, output peak {:.4}", snapshots, peak);
    println!("total per-block node time: {} ns (sum of all nodes)\n", total);
    println!("{:>4}  {:>8}  {:>6}  {:<14}  {}", "node", "ns/blk", "share", "type", "name");
    println!("{}", "-".repeat(60));
    let mut shown_share = 0.0f64;
    for (n, avg) in rows.iter().take(20) {
        let share = *avg as f64 / total.max(1) as f64 * 100.0;
        shown_share += share;
        let ty = idx_to_type.get(&(*n as u32)).map(|&t| type_name(t)).unwrap_or("?");
        let name = idx_to_name.get(&(*n as u32)).map(|s| s.as_str()).unwrap_or("");
        println!("{:>4}  {:>8}  {:>5.1}%  {:<14}  {}", n, avg, share, ty, name);
    }
    println!("{}", "-".repeat(60));
    println!("top 20 account for {:.1}% of node time", shown_share);

    // Aggregate by processor type — the actionable view.
    let mut by_type: HashMap<&'static str, (u64, u32)> = HashMap::new();
    for (n, avg) in &rows {
        let ty = idx_to_type.get(&(*n as u32)).map(|&t| type_name(t)).unwrap_or("?");
        let e = by_type.entry(ty).or_insert((0, 0));
        e.0 += *avg;
        e.1 += 1;
    }
    let mut type_rows: Vec<(&str, u64, u32)> = by_type.iter().map(|(k, v)| (*k, v.0, v.1)).collect();
    type_rows.sort_by(|a, b| b.1.cmp(&a.1));
    println!("\nby processor type:");
    println!("{:>8}  {:>6}  {:>5}  {}", "ns/blk", "share", "count", "type");
    for (ty, ns, cnt) in type_rows {
        println!("{:>8}  {:>5.1}%  {:>5}  {}", ns, ns as f64 / total.max(1) as f64 * 100.0, cnt, ty);
    }
}
