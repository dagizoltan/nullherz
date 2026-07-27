// Non-RT plane (test-harness pacing): thread sleep is sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]

//! The conductor's control path must cost the same for a six-minute track as
//! for a fifteen-second one.
//!
//! A library row carries the whole waveform (peaks, MIP pyramid, band
//! waveform) and is persisted as JSON, so `get_track` is linear in track
//! LENGTH: ~2.5 ms for a 17-second demo WAV, ~61 ms for a 6-minute release.
//! Three hot paths used to call it per deck regardless:
//!
//!   * `TelemetryService::update_timeline` — once per AUDIO BLOCK, to copy 64
//!     waveform floats into a telemetry field nothing read.
//!   * `Conductor::sync_sampler_metadata` — once a second, holding the
//!     `engine_handle` lock the backend needs to render every block.
//!   * `MixerOrchestrator::translate_command` — per deck load, for bpm/key/DNA.
//!
//! With two full-length tracks on decks A and B that came to 268 ms of work
//! per 5.8 ms audio block: the control thread ran ~46x behind realtime, sat on
//! the library mutex so `LoadTrackToDeck` could not get in, and stalled the
//! audio thread out of the engine lock for 130 ms at a time. First audio
//! arrived 11.6 s after Play. The console played the short demo WAVs fine and
//! looked incapable of playing real tracks.
//!
//! These assertions are against the AUDIO BLOCK BUDGET with wide headroom, so
//! a loaded CI box cannot flake them, but the 46x regression cannot return.

use std::sync::Arc;
use std::time::{Duration, Instant};

use nullherz_conductor::Conductor;
use nullherz_dna::GeneticLibrary;
use nullherz_traits::{Command, PerformanceCommand};

const SR: f64 = 44_100.0;
const BLOCK: usize = 256;

/// One audio block of wall time — the period the whole control path shares.
fn block_budget() -> Duration {
    Duration::from_secs_f64(BLOCK as f64 / SR)
}

fn pump_block(conductor: &mut Conductor, left: &mut [f32], right: &mut [f32]) {
    let inputs: Vec<&[f32]> = vec![];
    let mut outputs = vec![left, right];
    let mut lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
    let arc = lock.as_mut().expect("engine installed");
    let ptr = Arc::as_ptr(arc) as *mut dyn nullherz_traits::RenderingEngine;
    unsafe {
        (*ptr).process_block(&inputs, &mut outputs, BLOCK);
    }
}

/// Waveform metadata sized the way the analysis worker really produces it: one
/// peak per 128 samples, an 8-level MIP pyramid, and five band/envelope series
/// at the same resolution. For a 6-minute track that is ~1.6 million floats —
/// the payload that makes a library row expensive to read.
fn analysed_metadata(frames: u64) -> Arc<nullherz_traits::SampleMetadata> {
    let n = (frames / 128).max(1) as usize;
    let mip = |len: usize| {
        let mut levels = Vec::new();
        let mut l = len;
        for _ in 0..8 {
            levels.push(Arc::new(vec![0.25f32; l.max(1)]));
            l = (l / 2).max(1);
        }
        nullherz_traits::MipWaveform { levels }
    };

    let mut metadata = nullherz_traits::SampleMetadata::new_empty();
    metadata.bpm = 128.0;
    metadata.root_key = Some(5.0);
    metadata.total_samples = frames;
    metadata.channels = 2;
    metadata.peaks = Arc::new(vec![0.25f32; n]);
    metadata.mip_waveform = mip(n);
    metadata.band_waveform = nullherz_traits::BandWaveform {
        low: mip(n),
        mid: mip(n),
        high: mip(n),
        env_min: mip(n),
        env_max: mip(n),
    };
    Arc::new(metadata)
}

/// Register a playable track of `frames` frames with full analysis metadata,
/// in BOTH the registry and the library — the state a scanned, analysed and
/// once-loaded track is in. Planar stereo, so the sampler reads it for real.
fn register_track(conductor: &Conductor, id: u64, frames: u64) {
    let metadata = analysed_metadata(frames);
    let n = frames as usize;
    let mut buffer = Vec::with_capacity(n * 2);
    for _ in 0..2 {
        buffer.extend((0..n).map(|i| (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / SR as f32).sin() * 0.5));
    }
    conductor
        .transfusion_manager
        .sample_registry
        .register_with_metadata(id, Arc::new(buffer), metadata.clone());

    conductor
        .library
        .lock()
        .save_track(&nullherz_dna::LibraryTrack {
            id,
            path: format!("synthetic://{id}"),
            title: format!("track_{id}"),
            artist: "regression".into(),
            album: "regression".into(),
            genre: "test".into(),
            energy_level: 0.5,
            metadata,
        })
        .expect("in-memory library save cannot fail");
}

struct Costs {
    load: Duration,
    worst_tick: Duration,
    worst_telemetry: Duration,
    audible: bool,
}

/// Boot the console, load `frames`-long tracks onto decks A and B, play them,
/// and measure what the control path costs while audio runs.
fn measure(frames: u64) -> Costs {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    let mut cmds = Vec::new();
    for (i, deck) in ['A', 'B'].into_iter().enumerate() {
        let id = 5_000 + i as u64;
        register_track(&conductor, id, frames);
        cmds.push(Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: deck, sample_id: id }));
        cmds.push(Command::Performance(PerformanceCommand::PlayDeck { deck_id: deck }));
    }

    let mut left = vec![0.0f32; BLOCK];
    let mut right = vec![0.0f32; BLOCK];
    for _ in 0..256 {
        pump_block(&mut conductor, &mut left, &mut right);
    }

    let t0 = Instant::now();
    conductor.apply_mixer_commands(cmds);
    let load = t0.elapsed();

    // Two seconds of audio. Long enough that the once-a-second metadata sync
    // runs (cold and warm), which is where the engine-lock stall lived.
    let mut telemetry = nullherz_traits::telemetry::Telemetry::default();
    let mut costs = Costs { load, worst_tick: Duration::ZERO, worst_telemetry: Duration::ZERO, audible: false };
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut blocks = 0;
    while blocks < (2.0 * SR) as usize / BLOCK && Instant::now() < deadline {
        let t = Instant::now();
        conductor.tick();
        costs.worst_tick = costs.worst_tick.max(t.elapsed());

        left.fill(0.0);
        right.fill(0.0);
        pump_block(&mut conductor, &mut left, &mut right);
        if left.iter().chain(right.iter()).any(|v| v.abs() > 1e-4) {
            costs.audible = true;
        }

        let t = Instant::now();
        conductor.update_timeline(&mut telemetry);
        costs.worst_telemetry = costs.worst_telemetry.max(t.elapsed());
        blocks += 1;
    }
    costs
}

#[test]
fn test_long_track_does_not_stall_the_control_path() {
    // 6 minutes stereo — a normal DJ track, and ~21x the demo WAVs the console
    // was developed against.
    let long = measure(6 * 60 * SR as u64);
    let budget = block_budget();

    assert!(long.audible, "decks carrying a full-length track never produced audio");

    // update_timeline runs once per audio block: over budget means the control
    // thread can never drain telemetry, and it holds the library mutex the
    // command path needs. This measured 268 ms (46x budget) before the fix.
    assert!(
        long.worst_telemetry < budget,
        "update_timeline() took {:?} per telemetry frame, over the {:?} audio-block budget — \
         a per-block library read is back on the telemetry path",
        long.worst_telemetry, budget
    );

    // tick() takes the engine_handle lock that the backend needs to render
    // every block, so a slow tick is a hard audio dropout. 130 ms before.
    assert!(
        long.worst_tick < budget,
        "tick() took {:?}, over the {:?} audio-block budget — something slow is holding \
         the engine lock (the metadata sync used to read the library under it)",
        long.worst_tick, budget
    );
}

#[test]
fn test_control_path_cost_does_not_scale_with_track_length() {
    let short = measure(15 * SR as u64); // 15 s, like the bundled demo WAVs
    let long = measure(6 * 60 * SR as u64); // 6 min, a real track: 24x the audio

    // The control path must be flat in track length. Generous factors absorb
    // timer noise on a loaded CI box; the regression this guards was 100x.
    let ratio = |l: Duration, s: Duration| l.as_secs_f64() / s.as_secs_f64().max(1e-6);

    assert!(
        long.worst_telemetry < short.worst_telemetry.max(Duration::from_micros(50)) * 10,
        "update_timeline() cost scales with track length: {:?} for 6 min vs {:?} for 15 s ({:.1}x \
         for 24x the audio)",
        long.worst_telemetry, short.worst_telemetry, ratio(long.worst_telemetry, short.worst_telemetry)
    );
    assert!(
        long.load < short.load.max(Duration::from_millis(5)) * 10,
        "LoadTrackToDeck cost scales with track length: {:?} for 6 min vs {:?} for 15 s ({:.1}x \
         for 24x the audio)",
        long.load, short.load, ratio(long.load, short.load)
    );
}
