//! Golden STEREO master render: the full 4-deck console, driven
//! deterministically, is compared sample-for-sample against a committed
//! reference render and must stay within an AUDIBLE tolerance. Any real
//! change to the chain — a pan law, a gain stage, a COLA window, a wiring
//! edit — moves the master far past that floor and fails loudly, and must be
//! acknowledged by regenerating the reference in the same commit.
//!
//! Why a tolerance, not an exact hash: adding nodes to the graph reorders the
//! float adds in the master's own SIMD summing, shifting the render by an
//! INAUDIBLE amount (~-104 dBFS, measured at ~6e-6 peak — below the 16-bit
//! floor). An exact bit-hash flips on that reassociation even though nothing
//! audible changed, so it cried wolf on every node-adding feature (see the
//! cue-bus fix, PR #337). The tolerance ignores reassociation and still
//! catches anything a listener could hear. Structural breaks (folds, dropped
//! wires, multi-producer buffers) are covered exactly by the mixer's
//! `test_bootstrap_has_single_producer_per_buffer` / connectivity tests;
//! comparing L and R planes SEPARATELY here keeps swap/fold detection too.
//!
//! Program material: each deck plays a MULTITONE stack (220 Hz – 13.2 kHz,
//! distinct frequency sets per channel), not a single sine. The reference can
//! only defend spectrum it contains — with four low sines, a master-EQ change
//! at 5 kHz would have been invisible. The window is ~3 beats at 120 BPM so
//! beat-aligned behavior (sequencer triggers, groove) is pinned, not just
//! steady-state tone.
//!
//! Deliberate property: sample-wise comparison means a LATENCY change in the
//! master path (even one sample) fails loudly — shifted audio diffs at
//! near-signal level. On a DJ console beat alignment is behavior, not noise;
//! an intended latency change goes through the same listen-then-regen ritual.
//!
//! Determinism: no backend thread, no conductor tick — the test drives
//! `process_block` directly like the offline bounce path, so every block
//! boundary, command arrival and transport step is identical on every run.
//! The render is bit-identical run to run on the same binary; the tolerance
//! only forgives drift ACROSS code changes (and across compilers/CPUs).
//!
//! Regenerate the reference after an INTENDED, listened-to sonic change:
//!   GOLDEN_REGEN=1 cargo test -p nullherz-conductor --test golden_master_render_test test_golden
//! It prints the old-vs-new delta in dBFS — quote it in the commit that
//! ships the new fixture, so the accepted change has a recorded magnitude.

use std::sync::Arc;

use nullherz_conductor::Conductor;
use nullherz_dna::GeneticLibrary;
use nullherz_traits::{Command, PerformanceCommand};

/// Peak per-sample |difference| we treat as AUDIBLE, in full-scale units
/// (1.0 == 0 dBFS). 1e-3 is -60 dBFS. Inaudible float reassociation from
/// adding graph nodes sits ~44 dB below this (~6e-6, -104 dBFS), so it passes;
/// any real DSP change (pan law, gain, EQ) is tens of dB above, so it fails.
const AUDIBLE_PEAK_DIFF: f32 = 1e-3;

const SR: f32 = 44_100.0;
const BLOCK: usize = 256;
/// Fixed pre-roll: enough blocks to stream-install the whole bootstrap
/// topology (bounded mutations per block) with headroom. Fixed, not
/// condition-based, so the timeline is identical on every run.
const INSTALL_BLOCKS: usize = 256;
/// Blocks between issuing Load/Play and the start of the compared render
/// (command delivery + trigger, fixed for determinism).
const ARM_BLOCKS: usize = 8;
/// Compared render length: ~1.49 s ≈ 3 beats at 120 BPM, so beat-aligned
/// behavior (sequencer triggers, groove) is inside the pinned window.
const RENDER_BLOCKS: usize = 256;

/// Committed reference master render: RENDER_BLOCKS*BLOCK f32 of the L plane
/// followed by the same count for the R plane, little-endian. Read at test
/// time; (re)written by GOLDEN_REGEN=1.
const FIXTURE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/golden_master.f32");

/// Largest |difference| and where it happened, plus the RMS of the
/// difference — peak gates the assertion, RMS contextualizes the failure.
struct PlaneDelta {
    peak: f32,
    peak_idx: usize,
    rms: f32,
}

fn plane_delta(live: &[f32], reference: &[f32]) -> PlaneDelta {
    let mut peak = 0.0f32;
    let mut peak_idx = 0usize;
    let mut sum_sq = 0.0f64;
    for (i, (&a, &b)) in live.iter().zip(reference).enumerate() {
        let d = (a - b).abs();
        if d > peak {
            peak = d;
            peak_idx = i;
        }
        sum_sq += (d as f64) * (d as f64);
    }
    let rms = (sum_sq / live.len().max(1) as f64).sqrt() as f32;
    PlaneDelta { peak, peak_idx, rms }
}

fn dbfs(x: f32) -> f32 {
    if x > 0.0 { 20.0 * x.log10() } else { f32::NEG_INFINITY }
}

fn decode_reference(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Per-partial amplitudes for the multitone stacks. Sum is 0.5, matching the
/// old single-sine peak so bus/limiter headroom is unchanged.
const PARTIAL_AMPS: [f32; 3] = [0.275, 0.15, 0.075];

/// Register a deterministic planar stereo MULTITONE: three partials per
/// channel spanning low/mid/high so the reference has energy across the band
/// (a single sine cannot defend spectrum it doesn't contain). Distinct L/R
/// frequency sets so a channel swap or fold changes both planes.
fn register_stereo_tone(conductor: &Conductor, id: u64, left_hz: [f32; 3], right_hz: [f32; 3]) {
    let frames = (SR * 2.0) as usize;
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

    // Low / mid / high partials per channel, all sets disjoint: full-band
    // coverage AND per-channel identity.
    register_stereo_tone(&conductor, 5_501, [220.0, 1_470.0, 6_300.0], [330.0, 2_210.0, 9_500.0]);
    register_stereo_tone(&conductor, 5_502, [440.0, 3_150.0, 11_000.0], [550.0, 4_400.0, 13_200.0]);

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

    // 3. Render, capturing L and R planes.
    let mut render_l = Vec::with_capacity(RENDER_BLOCKS * BLOCK);
    let mut render_r = Vec::with_capacity(RENDER_BLOCKS * BLOCK);
    for _ in 0..RENDER_BLOCKS {
        pump_block(&mut conductor, &mut left, &mut right);
        render_l.extend_from_slice(&left);
        render_r.extend_from_slice(&right);
    }

    // A reference of silence pins nothing: both sides must be hot.
    let peak_l = render_l.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
    let peak_r = render_r.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
    assert!(peak_l > 1e-4, "master L silent during golden render (peak {:.6})", peak_l);
    assert!(peak_r > 1e-4, "master R silent during golden render (peak {:.6})", peak_r);

    let n = RENDER_BLOCKS * BLOCK;

    // REGEN: report the magnitude of the change being accepted (old vs new),
    // then write the freshly rendered planes as the new reference and stop.
    if std::env::var("GOLDEN_REGEN").is_ok() {
        match std::fs::read(FIXTURE_PATH) {
            Ok(old) if old.len() == n * 2 * 4 => {
                let old_f32 = decode_reference(&old);
                let (old_l, old_r) = old_f32.split_at(n);
                let dl = plane_delta(&render_l, old_l);
                let dr = plane_delta(&render_r, old_r);
                eprintln!(
                    "GOLDEN_REGEN: change vs outgoing reference — \
                     L peak {:.2e} ({:.1} dBFS) rms {:.2e} ({:.1} dBFS); \
                     R peak {:.2e} ({:.1} dBFS) rms {:.2e} ({:.1} dBFS). \
                     Quote this in the commit.",
                    dl.peak, dbfs(dl.peak), dl.rms, dbfs(dl.rms),
                    dr.peak, dbfs(dr.peak), dr.rms, dbfs(dr.rms),
                );
            }
            Ok(old) if !old.is_empty() => {
                eprintln!("GOLDEN_REGEN: outgoing reference has unexpected size {}; replacing.", old.len());
            }
            _ => eprintln!("GOLDEN_REGEN: no usable outgoing reference; writing the first one."),
        }
        let mut bytes = Vec::with_capacity((render_l.len() + render_r.len()) * 4);
        for &s in render_l.iter().chain(render_r.iter()) {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(FIXTURE_PATH, &bytes).expect("write golden reference fixture");
        eprintln!("GOLDEN_REGEN: wrote {} bytes to {}", bytes.len(), FIXTURE_PATH);
        return;
    }

    // 4. Compare each plane against the committed reference within tolerance.
    let reference = std::fs::read(FIXTURE_PATH).unwrap_or_else(|e| {
        panic!(
            "missing golden reference fixture {}: {}. Generate it with \
             GOLDEN_REGEN=1 and commit it.",
            FIXTURE_PATH, e
        )
    });
    assert_eq!(
        reference.len(),
        n * 2 * 4,
        "reference fixture is {} bytes, expected {} ({} f32 x 2 planes). \
         Regenerate with GOLDEN_REGEN=1.",
        reference.len(), n * 2 * 4, n
    );
    let ref_f32 = decode_reference(&reference);
    let (ref_l, ref_r) = ref_f32.split_at(n);

    let dl = plane_delta(&render_l, ref_l);
    let dr = plane_delta(&render_r, ref_r);
    assert!(
        dl.peak < AUDIBLE_PEAK_DIFF,
        "master L diverged from reference: peak {:.2e} ({:.1} dBFS) at sample {}, \
         rms {:.2e} ({:.1} dBFS); audible floor {:.1} dBFS. If this is an \
         INTENDED sonic change, listen, then regenerate the fixture with \
         GOLDEN_REGEN=1 in this commit.",
        dl.peak, dbfs(dl.peak), dl.peak_idx, dl.rms, dbfs(dl.rms), dbfs(AUDIBLE_PEAK_DIFF)
    );
    assert!(
        dr.peak < AUDIBLE_PEAK_DIFF,
        "master R diverged from reference: peak {:.2e} ({:.1} dBFS) at sample {}, \
         rms {:.2e} ({:.1} dBFS); audible floor {:.1} dBFS. If this is an \
         INTENDED sonic change, listen, then regenerate the fixture with \
         GOLDEN_REGEN=1 in this commit.",
        dr.peak, dbfs(dr.peak), dr.peak_idx, dr.rms, dbfs(dr.rms), dbfs(AUDIBLE_PEAK_DIFF)
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

    register_stereo_tone(&conductor, 5_601, [220.0, 1_470.0, 6_300.0], [330.0, 2_210.0, 9_500.0]);

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

/// The mastering EQ is LIVE end to end: a SetParam to the named "master_eq"
/// node through the real command bus must audibly reshape the master render
/// (the golden test above proves the untouched stage changes nothing).
#[test]
fn test_master_eq_setparam_audibly_shapes_master() {
    let render = |low_gain: Option<f32>| -> Vec<f32> {
        let mut conductor = Conductor::with_library_path(":memory:");
        conductor.setup_engine();
        conductor.bootstrap_4channel_mixer();
        register_stereo_tone(&conductor, 5_701, [220.0, 1_470.0, 6_300.0], [330.0, 2_210.0, 9_500.0]);

        let mut left = vec![0.0f32; BLOCK];
        let mut right = vec![0.0f32; BLOCK];
        for _ in 0..INSTALL_BLOCKS {
            pump_block(&mut conductor, &mut left, &mut right);
        }

        let mut commands = vec![
            Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: 5_701 }),
            Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
        ];
        if let Some(gain) = low_gain {
            let eq_node = *conductor
                .mixer_manager
                .node_names
                .get("master_eq")
                .expect("bootstrap must name the master EQ");
            commands.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                target_id: eq_node as u64,
                param_id: 0, // LOW shelf
                value: gain,
                ramp_duration_samples: 0,
            }));
        }
        conductor.apply_mixer_commands(commands);
        for _ in 0..ARM_BLOCKS {
            pump_block(&mut conductor, &mut left, &mut right);
        }

        let mut out = Vec::with_capacity(64 * BLOCK);
        for _ in 0..64 {
            pump_block(&mut conductor, &mut left, &mut right);
            out.extend_from_slice(&left);
        }
        out
    };

    let flat = render(None);
    let cut = render(Some(0.25)); // low shelf -12 dB

    let peak_flat = flat.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
    assert!(peak_flat > 1e-4, "flat render silent (peak {:.6})", peak_flat);

    let max_diff = flat
        .iter()
        .zip(&cut)
        .fold(0.0f32, |m, (&a, &b)| m.max((a - b).abs()));
    assert!(
        max_diff > AUDIBLE_PEAK_DIFF,
        "a -12 dB low-shelf SetParam to master_eq changed the master by only \
         {:.2e} ({:.1} dBFS) — the EQ is not live in the chain",
        max_diff,
        dbfs(max_diff)
    );
}
