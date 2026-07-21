//! Background hydration: loading a track whose sample must be decoded from
//! disk must NOT block the command path, and the deck must still become
//! audible once the decode lands.
//!
//! The old inline decode froze the tick thread (and every queued command,
//! including the user's Play) for the full decode — ~4 s for a 5-minute WAV
//! in a debug build. This test drives the real path: a genuine WAV on disk,
//! a library row pointing at it, an EMPTY registry entry, Load+Play in one
//! batch, and then tick()+pump until the master goes hot.
#![allow(clippy::disallowed_methods)]

use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nullherz_conductor::Conductor;
use nullherz_dna::GeneticLibrary;
use nullherz_traits::{Command, PerformanceCommand};

const SR: u32 = 44_100;
const BLOCK: usize = 256;
/// Long enough that an inline decode would take well over the latency budget
/// asserted below, even on a fast machine (3 minutes ≈ 2.4 s inline decode
/// in a debug build).
const TRACK_SECS: u32 = 180;
/// The Load+Play batch must return within this budget. Inline decode of the
/// track above blows through it by an order of magnitude.
const APPLY_BUDGET: Duration = Duration::from_millis(500);

fn write_test_wav(path: &std::path::Path) {
    let n = (SR * TRACK_SECS) as usize;
    let data_len = (n * 4) as u32; // stereo 16-bit
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVE").unwrap();
    f.write_all(b"fmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
    f.write_all(&2u16.to_le_bytes()).unwrap(); // stereo
    f.write_all(&SR.to_le_bytes()).unwrap();
    f.write_all(&(SR * 4).to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    for i in 0..n {
        let v = ((i as f32 * 220.0 * 2.0 * std::f32::consts::PI / SR as f32).sin() * 16_000.0) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&v.to_le_bytes()).unwrap();
    }
}

fn pump_block(conductor: &mut Conductor, left: &mut [f32], right: &mut [f32]) {
    let inputs: Vec<&[f32]> = vec![];
    let mut outputs = vec![left, right];
    let mut lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
    let arc = lock.as_mut().expect("engine installed");
    let ptr = Arc::as_ptr(arc) as *mut dyn nullherz_traits::RenderingEngine;
    unsafe { (*ptr).process_block(&inputs, &mut outputs, BLOCK); }
}

#[test]
fn test_load_does_not_block_and_deck_still_sounds() {
    // tick()'s matchmaking path calls tokio::spawn; the app's tick thread
    // runs inside a runtime context, so the test provides one too.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let wav_path = std::env::temp_dir().join(format!("nullherz_hydration_test_{}.wav", std::process::id()));
    write_test_wav(&wav_path);

    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    // A library row pointing at the file, with NO registry entry: exactly the
    // state a scanned-but-never-played track is in.
    let track_id = 7_301u64;
    let mut metadata = nullherz_traits::SampleMetadata::new_empty();
    metadata.bpm = 120.0;
    conductor.library.lock().save_track(&nullherz_dna::LibraryTrack {
        id: track_id,
        path: wav_path.to_string_lossy().into_owned(),
        title: "hydration test".into(),
        artist: "t".into(),
        album: "t".into(),
        genre: "t".into(),
        energy_level: 0.5,
        metadata: Arc::new(metadata),
    }).unwrap();
    assert!(conductor.transfusion_manager.sample_registry.get(track_id).is_none());

    let mut left = vec![0.0f32; BLOCK];
    let mut right = vec![0.0f32; BLOCK];
    for _ in 0..256 {
        pump_block(&mut conductor, &mut left, &mut right);
    }

    // The user's gesture: Load then Play, back to back. This must return
    // immediately — the decode belongs to a background thread.
    let t0 = Instant::now();
    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: track_id }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
    ]);
    let apply_took = t0.elapsed();
    assert!(
        apply_took < APPLY_BUDGET,
        "Load+Play blocked the command path for {:?} (budget {:?}) — decode is back on the apply path",
        apply_took, APPLY_BUDGET
    );
    assert!(
        conductor.hydration_pending.contains(&track_id),
        "load must be tracked as an in-flight hydration"
    );

    // Drive tick + audio until the background decode lands and the deck goes
    // hot. tick() drains the completion and re-issues the load; the sampler's
    // pending_play then fires the held Play trigger.
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut peak = 0.0f32;
    while Instant::now() < deadline {
        conductor.tick();
        for _ in 0..8 {
            pump_block(&mut conductor, &mut left, &mut right);
        }
        peak = left.iter().chain(right.iter()).fold(peak, |a, &v| a.max(v.abs()));
        if peak > 1e-3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let _ = std::fs::remove_file(&wav_path);

    assert!(
        peak > 1e-3,
        "deck never became audible after background hydration (peak {:.6})",
        peak
    );
    assert!(
        !conductor.hydration_pending.contains(&track_id),
        "completed hydration must be cleared from the pending set"
    );
}

/// Loading the same still-decoding track twice must not spawn a second
/// decode; loading a DIFFERENT track on the same deck mid-decode must win —
/// the stale completion re-drives nothing.
#[test]
fn test_reload_during_decode_is_deduped_and_superseded() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let wav_path = std::env::temp_dir().join(format!("nullherz_hydration_dedupe_{}.wav", std::process::id()));
    write_test_wav(&wav_path);

    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    let slow_id = 7_401u64;
    let mut metadata = nullherz_traits::SampleMetadata::new_empty();
    metadata.bpm = 120.0;
    conductor.library.lock().save_track(&nullherz_dna::LibraryTrack {
        id: slow_id,
        path: wav_path.to_string_lossy().into_owned(),
        title: "slow".into(), artist: "t".into(), album: "t".into(), genre: "t".into(),
        energy_level: 0.5, metadata: Arc::new(metadata),
    }).unwrap();

    // An instantly-available tone registered directly (no decode needed).
    let fast_id = 7_402u64;
    let frames = (SR / 2) as usize;
    let tone: Vec<f32> = (0..frames * 2)
        .map(|i| ((i % frames) as f32 * 330.0 * 2.0 * std::f32::consts::PI / SR as f32).sin() * 0.5)
        .collect();
    let mut md = nullherz_traits::SampleMetadata::new_empty();
    md.bpm = 120.0;
    md.total_samples = frames as u64;
    md.channels = 2;
    conductor.transfusion_manager.sample_registry.register_with_metadata(fast_id, Arc::new(tone), Arc::new(md));

    let mut left = vec![0.0f32; BLOCK];
    let mut right = vec![0.0f32; BLOCK];
    for _ in 0..256 {
        pump_block(&mut conductor, &mut left, &mut right);
    }

    // Double-click Load: second apply must dedupe against the in-flight decode.
    conductor.apply_mixer_commands(vec![Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: slow_id })]);
    conductor.apply_mixer_commands(vec![Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: slow_id })]);
    assert!(conductor.hydration_pending.contains(&slow_id));

    // The user changes their mind mid-decode: deck A now maps to fast_id.
    conductor.apply_mixer_commands(vec![Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: fast_id })]);
    assert_eq!(conductor.mixer_manager.deck_samples.get(&'A'), Some(&fast_id));

    // Let the slow decode finish and its completion drain.
    let deadline = Instant::now() + Duration::from_secs(30);
    while conductor.hydration_pending.contains(&slow_id) && Instant::now() < deadline {
        conductor.tick();
        for _ in 0..4 {
            pump_block(&mut conductor, &mut left, &mut right);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = std::fs::remove_file(&wav_path);

    assert!(!conductor.hydration_pending.contains(&slow_id), "decode never completed");
    // The stale completion must NOT have clobbered the newer load.
    assert_eq!(
        conductor.mixer_manager.deck_samples.get(&'A'),
        Some(&fast_id),
        "a stale hydration completion re-drove a superseded deck load"
    );
    // The decoded sample still landed in the registry for future use.
    assert!(conductor.transfusion_manager.sample_registry.get(slow_id).is_some());
}
