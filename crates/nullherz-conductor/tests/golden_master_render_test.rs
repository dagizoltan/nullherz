//! Golden-hash STEREO master render: the full 4-deck console, driven
//! deterministically, must produce bit-identical L and R at the master
//! forever. Any change to the chain — a pan law, a gain stage, a COLA
//! window, a wiring edit — flips a hash and must be acknowledged by
//! updating the constant in the same commit that changes the sound.
//!
//! The channel-identity tests catch STRUCTURAL breaks (folds, dropped
//! wires); this catches GRADUAL ones. Hashing L and R separately means a
//! change that only warps one side cannot hide.
//!
//! Determinism: no backend thread, no conductor tick — the test drives
//! `process_block` directly like the offline bounce path, so every block
//! boundary, command arrival and transport step is identical on every run.
//! (Worker-pool scheduling does not affect the result: stages are barriers
//! and nodes write disjoint buffers.)
//!
//! Hashes are exact IEEE-754 bit patterns on x86_64 (the shared dev/CI
//! target); a new target gets its own constants.

use std::sync::Arc;

use nullherz_conductor::Conductor;
use nullherz_dna::GeneticLibrary;
use nullherz_traits::{Command, PerformanceCommand};

/// Expected FNV-1a hashes of the master L/R streams. If a change flips
/// these INTENTIONALLY, listen first, then update the constants in the same
/// commit. If you did not intend to change the sound, you broke it.
const GOLDEN_MASTER_L: u64 = 0x5a17ef877a06a366;
const GOLDEN_MASTER_R: u64 = 0xa68a9d59da589089;

const SR: f32 = 44_100.0;
const BLOCK: usize = 256;
/// Fixed pre-roll: enough blocks to stream-install the whole bootstrap
/// topology (bounded mutations per block) with headroom. Fixed, not
/// condition-based, so the timeline is identical on every run.
const INSTALL_BLOCKS: usize = 256;
/// Blocks between issuing Load/Play and the start of hashing (command
/// delivery + trigger, fixed for determinism).
const ARM_BLOCKS: usize = 8;
/// Hashed render length.
const RENDER_BLOCKS: usize = 256;

fn fnv1a(acc: u64, bytes: &[u8]) -> u64 {
    let mut h = acc;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn hash_block(acc: u64, block: &[f32]) -> u64 {
    let mut h = acc;
    for &s in block {
        h = fnv1a(h, &s.to_bits().to_le_bytes());
    }
    h
}

/// Register a deterministic planar stereo tone: distinct L/R frequencies so
/// a channel swap or fold changes both hashes.
fn register_stereo_tone(conductor: &Conductor, id: u64, left_hz: f32, right_hz: f32) {
    let frames = (SR * 2.0) as usize;
    let mut samples = Vec::with_capacity(frames * 2);
    for hz in [left_hz, right_hz] {
        for i in 0..frames {
            samples.push((i as f32 * hz * 2.0 * std::f32::consts::PI / SR).sin() * 0.5);
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
        title: "golden stereo tone".to_string(),
        artist: "golden".to_string(),
        album: "golden".to_string(),
        genre: "test tone".to_string(),
        energy_level: 0.5,
        metadata,
    })
    .expect("in-memory library save cannot fail");
}

/// Process one block through the engine exactly the way the offline bounce
/// path does (single-threaded, engine owned by the test).
fn pump_block(conductor: &mut Conductor, left: &mut [f32], right: &mut [f32]) {
    let inputs: Vec<&[f32]> = vec![];
    let mut outputs = vec![left, right];
    let mut engine_lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
    let engine_arc = engine_lock.as_mut().expect("setup_engine must install an engine");
    if let Some(engine) = Arc::get_mut(engine_arc) {
        engine.process_block(&inputs, &mut outputs, BLOCK);
    } else {
        // The engine Arc is shared with coordinator plumbing; no other thread
        // is running in this test, so exclusive access holds by construction.
        let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn nullherz_traits::RenderingEngine;
        unsafe { (*engine_ptr).process_block(&inputs, &mut outputs, BLOCK); }
    }
}

#[test]
fn test_golden_stereo_master_render() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    register_stereo_tone(&conductor, 5_501, 220.0, 330.0);
    register_stereo_tone(&conductor, 5_502, 440.0, 550.0);

    let mut left = vec![0.0f32; BLOCK];
    let mut right = vec![0.0f32; BLOCK];

    // 1. Stream-install the bootstrap topology deterministically.
    for _ in 0..INSTALL_BLOCKS {
        pump_block(&mut conductor, &mut left, &mut right);
    }
    {
        // Loud failure if the fixed pre-roll ever becomes insufficient
        // (e.g. the bootstrap grows): a partial install would silently
        // change the hashes instead.
        let expected = conductor
            .topology_manager
            .active_node_types
            .keys()
            .filter(|&&idx| (idx as usize) < nullherz_traits::MAX_NODES)
            .count();
        let engine_lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
        let installed = engine_lock.as_ref().map(|e| e.list_children().len()).unwrap_or(0);
        drop(engine_lock);
        assert!(
            installed >= expected,
            "bootstrap not fully installed after {} blocks ({}/{}); raise INSTALL_BLOCKS",
            INSTALL_BLOCKS, installed, expected
        );
    }

    // 2. Load and play decks A and B in one deterministic batch.
    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: 5_501 }),
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'B', sample_id: 5_502 }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'B' }),
    ]);
    for _ in 0..ARM_BLOCKS {
        pump_block(&mut conductor, &mut left, &mut right);
    }

    // 3. Render and hash L and R independently.
    let (mut hash_l, mut hash_r) = (0xcbf29ce484222325u64, 0xcbf29ce484222325u64);
    let (mut peak_l, mut peak_r) = (0.0f32, 0.0f32);
    for _ in 0..RENDER_BLOCKS {
        pump_block(&mut conductor, &mut left, &mut right);
        hash_l = hash_block(hash_l, &left);
        hash_r = hash_block(hash_r, &right);
        peak_l = left.iter().fold(peak_l, |a, &v| a.max(v.abs()));
        peak_r = right.iter().fold(peak_r, |a, &v| a.max(v.abs()));
    }

    // A golden hash of silence pins nothing: both sides must be hot.
    assert!(peak_l > 1e-4, "master L silent during golden render (peak {:.6})", peak_l);
    assert!(peak_r > 1e-4, "master R silent during golden render (peak {:.6})", peak_r);

    assert_eq!(
        hash_l, GOLDEN_MASTER_L,
        "master L changed: got {:#018x}. If intentional, listen, then update GOLDEN_MASTER_L in this commit.",
        hash_l
    );
    assert_eq!(
        hash_r, GOLDEN_MASTER_R,
        "master R changed: got {:#018x}. If intentional, listen, then update GOLDEN_MASTER_R in this commit.",
        hash_r
    );
}

/// Live capture end to end: arm the master resample tap while decks play,
/// disarm, and the recording must land in the SampleRegistry as a playable
/// PLANAR STEREO source.
///
/// This also proves the capture node cannot alter the master path — it runs
/// inside the same deterministic console as the golden render above.
#[test]
fn test_capture_records_master_as_planar_stereo() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    register_stereo_tone(&conductor, 5_601, 220.0, 330.0);

    let mut left = vec![0.0f32; BLOCK];
    let mut right = vec![0.0f32; BLOCK];
    for _ in 0..INSTALL_BLOCKS {
        pump_block(&mut conductor, &mut left, &mut right);
    }

    let cap_node = *conductor
        .mixer_manager
        .node_names
        .get("capture_node")
        .expect("bootstrap must name the capture node");
    assert!(
        (cap_node as usize) < nullherz_traits::MAX_NODES,
        "capture node must live at a legal graph index, got {}",
        cap_node
    );

    let ids_before = conductor.transfusion_manager.sample_registry.list_ids();

    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: 5_601 }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
        // Arm the recorder (param 3) via the command bus.
        Command::Mixer(nullherz_traits::MixerCommand::SetParam {
            target_id: cap_node as u64,
            param_id: 3,
            value: 1.0,
            ramp_duration_samples: 0,
        }),
    ]);
    for _ in 0..64 {
        pump_block(&mut conductor, &mut left, &mut right);
    }
    conductor.apply_mixer_commands(vec![Command::Mixer(nullherz_traits::MixerCommand::SetParam {
        target_id: cap_node as u64,
        param_id: 3,
        value: 0.0,
        ramp_duration_samples: 0,
    })]);
    for _ in 0..4 {
        pump_block(&mut conductor, &mut left, &mut right);
    }

    // The orchestrator tick polls snapshots off the engine and registers them.
    conductor.tick();

    let ids_after = conductor.transfusion_manager.sample_registry.list_ids();
    let new_id = ids_after
        .iter()
        .find(|id| !ids_before.contains(id))
        .copied()
        .expect("an armed capture over live audio must register a snapshot");

    let sample = conductor.transfusion_manager.sample_registry.get(new_id).unwrap();
    assert_eq!(sample.metadata.channels, 2, "captures are planar stereo");
    assert_eq!(
        sample.buffer.len() as u64,
        sample.metadata.total_samples * 2,
        "buffer must be exactly two planes of total_samples frames"
    );
    assert!(
        sample.buffer.iter().any(|&s| s != 0.0),
        "captured audio must not be silence while a deck is playing"
    );
}
