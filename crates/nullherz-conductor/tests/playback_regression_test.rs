// Non-RT plane (test-harness pacing): thread sleep is sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]

//! Full-chain playback regression test — headless, no audio hardware.
//!
//! The July 2026 silence marathon found 18 defects that were invisible to
//! every unit test because each one lived in the seams BETWEEN subsystems:
//! command translation, transport ordering, topology commit gating, registry
//! hydration, deck mapping. This test closes that class by driving the same
//! path a user's Play click takes — full conductor boot, real engine, real
//! backend thread — and asserting that audio actually comes out the master.
//!
//! Chain under test:
//!   bootstrap_4channel_mixer -> topology ring -> commit/install
//!   -> LoadTrackToDeck (registry) -> PlayDeck (Play + PlayNode)
//!   -> sampler -> strip -> bus -> crossfader -> master EQ -> limiter
//!   -> telemetry peak_levels
//!
//! Runs on the Threaded backend (software clock): CI-safe, no ALSA, no RT
//! privileges. Assertions are made in AUDIO time (sample_counter), so a slow
//! debug CI machine cannot turn a pass into a flake.

use std::sync::Arc;
use std::time::{Duration, Instant};

use nullherz_conductor::Conductor;
use nullherz_dna::GeneticLibrary;
use nullherz_traits::{AudioBackendType, Command, PerformanceCommand};

/// Peaks below this are treated as silence. The tone is registered at 0.5
/// amplitude; even after every legal gain stage in the default console the
/// master should sit orders of magnitude above this.
const AUDIBLE: f32 = 1e-4;

/// The deck must be audible within this much AUDIO time after Play.
const MAX_SAMPLES_TO_SOUND: u64 = 5 * 44_100;

/// Wall-clock cap so a silent engine fails the test instead of hanging it.
const WALL_DEADLINE: Duration = Duration::from_secs(30);

/// Generate a 2-second sine tone and register it the way the real pipeline
/// leaves tracks: buffer + metadata in the sample registry, row in the
/// library (so BPM auto-sync resolves). No file on disk is needed — the
/// registry entry short-circuits command-handler hydration.
fn register_tone(conductor: &Conductor, id: u64, freq: f32) {
    let sample_rate = 44_100.0f32;
    let len = (sample_rate * 2.0) as usize;
    let tone: Vec<f32> = (0..len)
        .map(|i| (i as f32 * freq * 2.0 * std::f32::consts::PI / sample_rate).sin() * 0.5)
        .collect();

    let mut metadata = nullherz_traits::SampleMetadata::new_empty();
    metadata.bpm = 120.0;
    metadata.total_samples = len as u64;
    let metadata = Arc::new(metadata);

    conductor
        .transfusion_manager
        .sample_registry
        .register_with_metadata(id, Arc::new(tone), metadata.clone());

    let lib = conductor.library.lock();
    lib.save_track(&nullherz_dna::LibraryTrack {
        id,
        path: format!("tone://{}", id),
        title: format!("tone_{}hz", freq as u32),
        artist: "regression".to_string(),
        album: "regression".to_string(),
        genre: "test tone".to_string(),
        energy_level: 0.5,
        metadata,
    })
    .expect("in-memory library save cannot fail");
}

/// Boot the full conductor, play a tone on `deck`, and return the maximum
/// peaks seen at the deck's sampler node and at the master limiter, plus how
/// much audio time passed between Play and the master going hot.
fn run_deck_playback(deck: char) -> (f32, f32, Option<u64>, u64) {
    let mut conductor = Conductor::with_library_path(":memory:");
    let mut context = conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    let tone_id = 9_000 + deck as u64;
    register_tone(&conductor, tone_id, 440.0);

    conductor
        .start_backend(AudioBackendType::Threaded)
        .expect("threaded backend must start without hardware");

    // Wait for the streamed bootstrap topology to be fully installed: every
    // AddNode the conductor issued must be a live child in the engine.
    // (Firing PlayNode into a half-built graph loses the fire-once broadcast
    // to DummyProcessors — see docs/system/ transport ordering note.)
    //
    // Nodes at indices >= MAX_NODES are excluded: the graph silently drops
    // them (known defect — the preview node sits at 111 with MAX_NODES = 64).
    let expected_nodes = conductor
        .topology_manager
        .active_node_types
        .keys()
        .filter(|&&idx| (idx as usize) < nullherz_traits::MAX_NODES)
        .count();
    let install_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let installed = {
            let handle = conductor.engine_coordinator.backend_manager.engine_handle.lock();
            handle.as_ref().map(|e| e.list_children().len()).unwrap_or(0)
        };
        if installed >= expected_nodes {
            break;
        }
        if Instant::now() >= install_deadline {
            let live: Vec<String> = {
                let handle = conductor.engine_coordinator.backend_manager.engine_handle.lock();
                handle
                    .as_ref()
                    .map(|e| {
                        e.list_children()
                            .iter()
                            .enumerate()
                            .map(|(i, c)| format!("{}:{}", i, c.processor_type()))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            panic!(
                "topology never finished installing: {}/{} nodes live after 10s; live nodes: {:?}",
                installed, expected_nodes, live
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Note the engine's audio clock before pressing Play.
    let mut counter_at_play = 0u64;
    while let Some(tel) = context.telemetry_consumer.pop() {
        counter_at_play = tel.sample_counter;
    }

    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: deck, sample_id: tone_id }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: deck }),
    ]);

    let sampler_idx = *conductor
        .mixer_manager
        .node_names
        .get(&format!("deck_{}_sampler", deck.to_lowercase()))
        .expect("bootstrap must name the deck sampler") as usize;
    let limiter_idx = *conductor
        .mixer_manager
        .node_names
        .get("master_limiter")
        .expect("bootstrap must name the master limiter") as usize;

    let mut sampler_peak = 0.0f32;
    let mut limiter_peak = 0.0f32;
    let mut samples_to_sound: Option<u64> = None;
    let mut frames = 0u64;
    let wall_deadline = Instant::now() + WALL_DEADLINE;

    while Instant::now() < wall_deadline {
        while let Some(tel) = context.telemetry_consumer.pop() {
            frames += 1;
            sampler_peak = sampler_peak.max(tel.peak_levels[sampler_idx]);
            let master = tel.peak_levels[limiter_idx];
            limiter_peak = limiter_peak.max(master);
            if master > AUDIBLE && samples_to_sound.is_none() {
                samples_to_sound = Some(tel.sample_counter.saturating_sub(counter_at_play));
            }
        }
        if samples_to_sound.is_some() && sampler_peak > AUDIBLE {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    conductor.stop_backend();
    (sampler_peak, limiter_peak, samples_to_sound, frames)
}

fn assert_deck_audible(deck: char) {
    let (sampler_peak, limiter_peak, samples_to_sound, frames) = run_deck_playback(deck);

    assert!(frames > 0, "no telemetry at all — the audio thread never ran");
    assert!(
        sampler_peak > AUDIBLE,
        "deck {} sampler stayed silent (peak {:.6}) — trigger chain broken before the strip",
        deck,
        sampler_peak
    );
    assert!(
        limiter_peak > AUDIBLE,
        "deck {} was hot at the sampler (peak {:.6}) but the master limiter stayed silent \
         (peak {:.6}) — signal lost between strip and master chain",
        deck,
        sampler_peak,
        limiter_peak
    );
    let samples = samples_to_sound.expect("master went hot, so time-to-sound must be recorded");
    assert!(
        samples <= MAX_SAMPLES_TO_SOUND,
        "deck {} took {} samples of audio time to reach the master (budget {})",
        deck,
        samples,
        MAX_SAMPLES_TO_SOUND
    );
}

/// Deck A: the canonical Library -> Load -> Play path must make sound.
#[test]
fn test_deck_a_playback_reaches_master() {
    assert_deck_audible('A');
}

/// Deck B: same chain through the other crossfader input and bus B.
#[test]
fn test_deck_b_playback_reaches_master() {
    assert_deck_audible('B');
}
