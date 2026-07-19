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
    register_tone_at_bpm(conductor, id, freq, 120.0)
}

/// As `register_tone`, but with an explicit BPM. The sampler's quantize path
/// derives its playback rate from transport BPM / track BPM, so decks carrying
/// different tempos exercise materially different code.
fn register_tone_at_bpm(conductor: &Conductor, id: u64, freq: f32, bpm: f32) {
    let sample_rate = 44_100.0f32;
    let len = (sample_rate * 2.0) as usize;
    let tone: Vec<f32> = (0..len)
        .map(|i| (i as f32 * freq * 2.0 * std::f32::consts::PI / sample_rate).sin() * 0.5)
        .collect();

    let mut metadata = nullherz_traits::SampleMetadata::new_empty();
    metadata.bpm = bpm;
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
    // The >= MAX_NODES filter is pure defense now: every bootstrap node,
    // including the preview sampler, has a legal index. (The preview node
    // used to sit at the LOGICAL id 111 and was silently dropped.)
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

/// Play several decks at once through one conductor and report, per deck, the
/// peak seen at its sampler — plus the master limiter peak.
///
/// Playing decks in isolation is not the same test: the July 2026 survival run
/// on ALSA had deck A hot and deck B silent with BOTH loaded and playing, which
/// isolation cannot reproduce. Bus summing, crossfader inputs and the shared
/// registry only interact when more than one deck is live.
fn run_multi_deck_playback(decks: &[char]) -> (Vec<(char, f32)>, f32) {
    let specs: Vec<(char, f32)> = decks.iter().map(|&d| (d, 120.0)).collect();
    run_multi_deck_playback_with_tempos(&specs)
}

/// Multi-deck playback where each deck carries its own tempo.
fn run_multi_deck_playback_with_tempos(specs: &[(char, f32)]) -> (Vec<(char, f32)>, f32) {
    let decks: Vec<char> = specs.iter().map(|(d, _)| *d).collect();
    let decks = &decks[..];
    let mut conductor = Conductor::with_library_path(":memory:");
    let mut context = conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    // Distinct tone per deck, so a deck cannot pass on another's signal.
    for (i, &(deck, bpm)) in specs.iter().enumerate() {
        register_tone_at_bpm(&conductor, 9_500 + deck as u64, 220.0 * (i as f32 + 1.0), bpm);
    }

    conductor
        .start_backend(AudioBackendType::Threaded)
        .expect("threaded backend must start without hardware");

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
        assert!(
            Instant::now() < install_deadline,
            "topology never finished installing: {}/{} nodes",
            installed,
            expected_nodes
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    // Load and play every deck in ONE command batch, the way the survival
    // harness (and a DJ hitting sync) does it.
    let mut commands = Vec::new();
    for &deck in decks {
        commands.push(Command::Performance(PerformanceCommand::LoadTrackToDeck {
            deck_id: deck,
            sample_id: 9_500 + deck as u64,
        }));
    }
    for &deck in decks {
        commands.push(Command::Performance(PerformanceCommand::PlayDeck { deck_id: deck }));
    }
    conductor.apply_mixer_commands(commands);

    let sampler_idx: Vec<(char, usize)> = decks
        .iter()
        .map(|&d| {
            (
                d,
                *conductor
                    .mixer_manager
                    .node_names
                    .get(&format!("deck_{}_sampler", d.to_lowercase()))
                    .expect("bootstrap must name every deck sampler") as usize,
            )
        })
        .collect();
    let limiter_idx = *conductor
        .mixer_manager
        .node_names
        .get("master_limiter")
        .expect("bootstrap must name the master limiter") as usize;

    let mut peaks: Vec<(char, f32)> = decks.iter().map(|&d| (d, 0.0f32)).collect();
    let mut limiter_peak = 0.0f32;
    let deadline = Instant::now() + WALL_DEADLINE;

    while Instant::now() < deadline {
        while let Some(tel) = context.telemetry_consumer.pop() {
            for (slot, (_, idx)) in peaks.iter_mut().zip(sampler_idx.iter()) {
                slot.1 = slot.1.max(tel.peak_levels[*idx]);
            }
            limiter_peak = limiter_peak.max(tel.peak_levels[limiter_idx]);
        }
        if peaks.iter().all(|(_, p)| *p > AUDIBLE) && limiter_peak > AUDIBLE {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    conductor.stop_backend();
    (peaks, limiter_peak)
}

/// Decks A and B share the crossfader but sit on opposite buses. Both must be
/// audible at once — that is what a DJ mix IS, and it is the exact case where
/// the survival run saw deck B drop out.
#[test]
fn test_decks_a_and_b_play_simultaneously() {
    let (peaks, limiter_peak) = run_multi_deck_playback(&['A', 'B']);

    for (deck, peak) in &peaks {
        assert!(
            *peak > AUDIBLE,
            "deck {} silent while playing alongside the others (peak {:.6}); \
             per-deck peaks: {:?}",
            deck,
            peak,
            peaks
        );
    }
    assert!(
        limiter_peak > AUDIBLE,
        "both decks were hot but the master limiter stayed silent (peak {:.6})",
        limiter_peak
    );
}

/// All four decks: bus A sums decks A+C, bus B sums decks B+D. A summing node
/// that dropped one of its inputs would leave exactly one deck of each pair
/// silent, which two decks cannot reveal.
#[test]
fn test_all_four_decks_play_simultaneously() {
    let (peaks, limiter_peak) = run_multi_deck_playback(&['A', 'B', 'C', 'D']);

    for (deck, peak) in &peaks {
        assert!(
            *peak > AUDIBLE,
            "deck {} silent with all four decks playing (peak {:.6}); \
             per-deck peaks: {:?}",
            deck,
            peak,
            peaks
        );
    }
    assert!(
        limiter_peak > AUDIBLE,
        "four decks hot but master limiter silent (peak {:.6})",
        limiter_peak
    );
}

/// The realistic DJ case, and the one the survival harness actually ran: two
/// decks at DIFFERENT tempos. Each LoadTrackToDeck emits SetBpm, so the
/// transport ends up on whichever track loaded last, and every other deck runs
/// at a quantize rate of transport_bpm / its own bpm. A deck must stay audible
/// when that ratio is not 1 — a phase-lock that walks the playhead off the end
/// of the buffer deactivates the voice permanently.
#[test]
fn test_decks_at_different_tempos_both_stay_audible() {
    // The survival harness's own pairing: 174 BPM neuro against 128 BPM house.
    let (peaks, limiter_peak) = run_multi_deck_playback_with_tempos(&[('A', 174.0), ('B', 128.0)]);

    for (deck, peak) in &peaks {
        assert!(
            *peak > AUDIBLE,
            "deck {} went silent playing against a different-tempo deck (peak {:.6}); \
             per-deck peaks: {:?}",
            deck,
            peak,
            peaks
        );
    }
    assert!(
        limiter_peak > AUDIBLE,
        "tempo-mismatched decks were hot but the master stayed silent (peak {:.6})",
        limiter_peak
    );
}

/// Register a PLANAR STEREO tone: channel 0 then channel 1, `channels` set.
fn register_stereo_tone(conductor: &Conductor, id: u64, left_hz: f32, right_hz: f32) {
    let sample_rate = 44_100.0f32;
    let frames = (sample_rate * 2.0) as usize;
    let mut samples = Vec::with_capacity(frames * 2);
    for hz in [left_hz, right_hz] {
        for i in 0..frames {
            samples.push((i as f32 * hz * 2.0 * std::f32::consts::PI / sample_rate).sin() * 0.5);
        }
    }

    let mut metadata = nullherz_traits::SampleMetadata::new_empty();
    metadata.bpm = 120.0;
    metadata.total_samples = frames as u64;
    metadata.channels = 2;
    let metadata = std::sync::Arc::new(metadata);

    conductor
        .transfusion_manager
        .sample_registry
        .register_with_metadata(id, std::sync::Arc::new(samples), metadata.clone());

    let lib = conductor.library.lock();
    lib.save_track(&nullherz_dna::LibraryTrack {
        id,
        path: format!("tone://{}", id),
        title: "stereo tone".to_string(),
        artist: "regression".to_string(),
        album: "regression".to_string(),
        genre: "test tone".to_string(),
        energy_level: 0.5,
        metadata,
    })
    .expect("in-memory library save cannot fail");
}

/// DNA groove transfusion must target the deck's LIVE sequencer node.
///
/// Regression: groove commands were aimed at NodeConventions sentinel ids
/// (70-73, all >= MAX_NODES) that no processor backed — the engine dropped
/// them silently and rhythmic transfusion did nothing. Deck sequencers are
/// real graph nodes now; the orchestrator resolves them by name.
#[test]
fn test_groove_commands_target_live_sequencer_nodes() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    // A track whose DNA carries a nonzero groove.
    let tone_id = 6_600;
    register_tone(&conductor, tone_id, 440.0);
    {
        let lib = conductor.library.lock();
        let mut track = lib.get_track(tone_id).unwrap().unwrap();
        let mut metadata = (*track.metadata).clone();
        metadata.dna.rhythmic.micro_timing[0] = 64;
        track.metadata = Arc::new(metadata);
        lib.save_track(&track).unwrap();
    }

    let seq_idx = *conductor
        .mixer_manager
        .node_names
        .get("deck_a_sequencer")
        .expect("bootstrap must name the deck sequencer");
    assert!(
        (seq_idx as usize) < nullherz_traits::MAX_NODES,
        "deck sequencer must live at a legal graph index, got {}",
        seq_idx
    );

    let cmd = Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: tone_id });
    let translated = nullherz_conductor::mixer_orchestrator::MixerOrchestrator::translate_command(
        &cmd,
        &conductor.mixer_manager,
        &conductor.library,
    );

    let groove_params: Vec<(u64, u32)> = translated
        .iter()
        .filter_map(|c| match c {
            Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, .. })
                if (100..356).contains(param_id) => Some((*target_id, *param_id)),
            _ => None,
        })
        .collect();

    assert!(!groove_params.is_empty(), "groove DNA must emit micro-timing commands");
    for (target, param) in &groove_params {
        assert_eq!(
            *target, seq_idx as u64,
            "groove param {} targets node {} instead of the live sequencer {}",
            param, target, seq_idx
        );
    }
}

/// Setting a hot cue must PERSIST: into the registry metadata (so the deck
/// picks it up) and into the library row (so it survives restart).
///
/// Regression: SetHotCue had no handler anywhere in the system — the UI sent
/// it, nothing consumed it, and cues silently resolved to their
/// 10%-of-track fallback positions forever.
#[test]
fn test_set_hot_cue_persists_to_registry_and_library() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    let tone_id = 4_400;
    register_tone(&conductor, tone_id, 440.0);

    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: tone_id }),
    ]);

    let sampler_idx = *conductor
        .mixer_manager
        .node_names
        .get("deck_a_sampler")
        .expect("bootstrap must name the deck sampler");

    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::SetHotCue {
            node_idx: sampler_idx,
            cue_idx: 3,
            position_samples: 12_345,
        }),
    ]);

    let registry_meta = conductor
        .transfusion_manager
        .sample_registry
        .get(tone_id)
        .expect("sample must stay registered")
        .metadata;
    assert_eq!(
        registry_meta.hot_cues[3],
        Some(12_345),
        "hot cue must land in the registry metadata"
    );

    let lib_meta = conductor
        .library
        .lock()
        .get_track(tone_id)
        .unwrap()
        .expect("track must exist in the library")
        .metadata;
    assert_eq!(
        lib_meta.hot_cues[3],
        Some(12_345),
        "hot cue must persist to the library row"
    );
}

/// Stereo sources must drive the deck chain exactly like mono ones.
///
/// Sample buffers are PLANAR (channel 0, then channel 1), and the sampler reads
/// frames-per-channel from metadata. A deck fed a stereo source must still
/// reach the master — if the layout were misread the playhead would run off the
/// end of what it thinks is the buffer and the voice would deactivate.
#[test]
fn test_stereo_source_plays_on_both_decks() {
    let mut conductor = Conductor::with_library_path(":memory:");
    let mut context = conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    for (i, deck) in ['A', 'B'].iter().enumerate() {
        register_stereo_tone(&conductor, 7_000 + *deck as u64, 220.0 * (i as f32 + 1.0), 330.0 * (i as f32 + 1.0));
    }

    conductor
        .start_backend(AudioBackendType::Threaded)
        .expect("threaded backend must start without hardware");

    let expected_nodes = conductor
        .topology_manager
        .active_node_types
        .keys()
        .filter(|&&idx| (idx as usize) < nullherz_traits::MAX_NODES)
        .count();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let installed = {
            let handle = conductor.engine_coordinator.backend_manager.engine_handle.lock();
            handle.as_ref().map(|e| e.list_children().len()).unwrap_or(0)
        };
        if installed >= expected_nodes { break; }
        assert!(Instant::now() < deadline, "topology never installed ({}/{})", installed, expected_nodes);
        std::thread::sleep(Duration::from_millis(10));
    }

    let mut commands = Vec::new();
    for deck in ['A', 'B'] {
        commands.push(Command::Performance(PerformanceCommand::LoadTrackToDeck {
            deck_id: deck,
            sample_id: 7_000 + deck as u64,
        }));
    }
    for deck in ['A', 'B'] {
        commands.push(Command::Performance(PerformanceCommand::PlayDeck { deck_id: deck }));
    }
    conductor.apply_mixer_commands(commands);

    let idx = |name: String| *conductor.mixer_manager.node_names.get(&name).expect("named node") as usize;
    let a = idx("deck_a_sampler".to_string());
    let b = idx("deck_b_sampler".to_string());
    let master = idx("master_limiter".to_string());

    let (mut a_peak, mut b_peak, mut master_peak) = (0.0f32, 0.0f32, 0.0f32);
    let deadline = Instant::now() + WALL_DEADLINE;
    while Instant::now() < deadline {
        while let Some(tel) = context.telemetry_consumer.pop() {
            a_peak = a_peak.max(tel.peak_levels[a]);
            b_peak = b_peak.max(tel.peak_levels[b]);
            master_peak = master_peak.max(tel.peak_levels[master]);
        }
        if a_peak > AUDIBLE && b_peak > AUDIBLE && master_peak > AUDIBLE { break; }
        std::thread::sleep(Duration::from_millis(10));
    }
    conductor.stop_backend();

    assert!(a_peak > AUDIBLE, "deck A silent on a stereo source (peak {:.6})", a_peak);
    assert!(b_peak > AUDIBLE, "deck B silent on a stereo source (peak {:.6})", b_peak);
    assert!(master_peak > AUDIBLE, "stereo decks hot but master silent (peak {:.6})", master_peak);
}

/// Peaks below this count as silence for the channel-identity tests. Stricter
/// than AUDIBLE: the silent side of a one-sided source is structurally zero
/// (separate buffers end to end), not merely quiet.
const SILENT: f32 = 1e-5;

/// Play a one-sided stereo tone on deck A and return the peaks observed at the
/// per-side master summing nodes (the last per-channel observation points)
/// plus the master limiter.
///
/// This is the test the strip wiring earns its keep by: a mono fold anywhere
/// in the chain leaks the hot side into the silent one, and a dropped right
/// channel leaves the right sum dead no matter what plays.
fn run_channel_identity(left_hz: f32, right_hz: f32) -> (f32, f32, f32) {
    let mut conductor = Conductor::with_library_path(":memory:");
    let mut context = conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    // A frequency of 0.0 renders sin(0) == 0.0 exactly: a structurally silent
    // plane, not just a quiet one.
    let tone_id = 7_700;
    register_stereo_tone(&conductor, tone_id, left_hz, right_hz);

    conductor
        .start_backend(AudioBackendType::Threaded)
        .expect("threaded backend must start without hardware");

    let expected_nodes = conductor
        .topology_manager
        .active_node_types
        .keys()
        .filter(|&&idx| (idx as usize) < nullherz_traits::MAX_NODES)
        .count();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let installed = {
            let handle = conductor.engine_coordinator.backend_manager.engine_handle.lock();
            handle.as_ref().map(|e| e.list_children().len()).unwrap_or(0)
        };
        if installed >= expected_nodes { break; }
        assert!(Instant::now() < deadline, "topology never installed ({}/{})", installed, expected_nodes);
        std::thread::sleep(Duration::from_millis(10));
    }

    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: tone_id }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
    ]);

    let idx = |name: &str| *conductor.mixer_manager.node_names.get(name).expect("named node") as usize;
    let sum_l = idx("master_sum_l");
    let sum_r = idx("master_sum_r");
    let limiter = idx("master_limiter");

    let (mut l_peak, mut r_peak, mut limiter_peak) = (0.0f32, 0.0f32, 0.0f32);
    let deadline = Instant::now() + WALL_DEADLINE;
    while Instant::now() < deadline {
        while let Some(tel) = context.telemetry_consumer.pop() {
            l_peak = l_peak.max(tel.peak_levels[sum_l]);
            r_peak = r_peak.max(tel.peak_levels[sum_r]);
            limiter_peak = limiter_peak.max(tel.peak_levels[limiter]);
        }
        // Wait for the HOT side; the silent side is asserted after the run so
        // any late bleed still fails the test.
        if l_peak.max(r_peak) > AUDIBLE && limiter_peak > AUDIBLE { break; }
        std::thread::sleep(Duration::from_millis(10));
    }
    conductor.stop_backend();
    (l_peak, r_peak, limiter_peak)
}

/// A left-only source must come out left-only: hot L sum, silent R sum.
/// Catches a dropped right wire (R never written) AND any L->R mono fold.
#[test]
fn test_left_only_source_stays_left() {
    let (l_peak, r_peak, limiter_peak) = run_channel_identity(440.0, 0.0);
    assert!(l_peak > AUDIBLE, "left-only source never reached the left master sum (peak {:.6})", l_peak);
    assert!(
        r_peak < SILENT,
        "left-only source leaked into the right channel (L {:.6}, R {:.6}) — a mono fold survives in the chain",
        l_peak, r_peak
    );
    assert!(limiter_peak > AUDIBLE, "left sum hot but master limiter silent (peak {:.6})", limiter_peak);
}

/// Library pre-listen must be audible: the Preview command routes a sample to
/// the preview node, which mixes into the master sums.
///
/// Regression: the preview node was created at NodeConventions::PREVIEW (111,
/// a LOGICAL id >= MAX_NODES), so the graph silently dropped it and preview
/// never made a sound. It now gets a real allocated index; the conductor
/// translates the sentinel.
#[test]
fn test_preview_command_is_audible() {
    let mut conductor = Conductor::with_library_path(":memory:");
    let mut context = conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    let tone_id = 8_800;
    register_tone(&conductor, tone_id, 440.0);

    conductor
        .start_backend(AudioBackendType::Threaded)
        .expect("threaded backend must start without hardware");

    let expected_nodes = conductor
        .topology_manager
        .active_node_types
        .keys()
        .filter(|&&idx| (idx as usize) < nullherz_traits::MAX_NODES)
        .count();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let installed = {
            let handle = conductor.engine_coordinator.backend_manager.engine_handle.lock();
            handle.as_ref().map(|e| e.list_children().len()).unwrap_or(0)
        };
        if installed >= expected_nodes { break; }
        assert!(Instant::now() < deadline, "topology never installed ({}/{})", installed, expected_nodes);
        std::thread::sleep(Duration::from_millis(10));
    }

    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::Preview { sample_id: tone_id }),
    ]);

    let preview_idx = *conductor
        .mixer_manager
        .node_names
        .get("preview_node")
        .expect("bootstrap must name the preview node") as usize;
    assert!(
        preview_idx < nullherz_traits::MAX_NODES,
        "preview node must live at a legal graph index, got {}",
        preview_idx
    );
    let limiter_idx = *conductor
        .mixer_manager
        .node_names
        .get("master_limiter")
        .expect("bootstrap must name the master limiter") as usize;

    let (mut preview_peak, mut limiter_peak) = (0.0f32, 0.0f32);
    let deadline = Instant::now() + WALL_DEADLINE;
    while Instant::now() < deadline {
        while let Some(tel) = context.telemetry_consumer.pop() {
            preview_peak = preview_peak.max(tel.peak_levels[preview_idx]);
            limiter_peak = limiter_peak.max(tel.peak_levels[limiter_idx]);
        }
        if preview_peak > AUDIBLE && limiter_peak > AUDIBLE { break; }
        std::thread::sleep(Duration::from_millis(10));
    }
    conductor.stop_backend();

    assert!(
        preview_peak > AUDIBLE,
        "preview node stayed silent (peak {:.6}) — Preview command chain broken",
        preview_peak
    );
    assert!(
        limiter_peak > AUDIBLE,
        "preview was hot (peak {:.6}) but never reached the master (peak {:.6})",
        preview_peak,
        limiter_peak
    );
}

/// A right-only source must come out right-only. This is the direction the
/// mono-era strip actually broke: every stage carried one buffer, so the
/// right plane died at the first hop.
#[test]
fn test_right_only_source_stays_right() {
    let (l_peak, r_peak, limiter_peak) = run_channel_identity(0.0, 440.0);
    assert!(r_peak > AUDIBLE, "right-only source never reached the right master sum (peak {:.6})", r_peak);
    assert!(
        l_peak < SILENT,
        "right-only source leaked into the left channel (L {:.6}, R {:.6}) — a mono fold survives in the chain",
        l_peak, r_peak
    );
    assert!(limiter_peak > AUDIBLE, "right sum hot but master limiter silent (peak {:.6})", limiter_peak);
}
