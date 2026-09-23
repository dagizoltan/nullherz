//! The pitch fader is varispeed, and key lock has to cancel it.
//!
//! Two independent sources move a deck's rate: tempo sync (`transport_bpm /
//! track_bpm`) and the operator's pitch fader. Key lock promises that engaging it
//! holds the track's original pitch while tempo moves, and it delivers that by
//! transposing the deck's vocoder by `-12*log2(total_rate)`.
//!
//! Before a fader existed, sync was the only source, so `key_lock_semitones`
//! returned early unless the deck was synced. That early return becomes a SILENT
//! failure the moment a fader exists: KEY LOCK lights up on a hand-pitched deck,
//! corrects by zero, and the pitch drops anyway — the opposite of what the latch
//! promises, with the indicator claiming otherwise.

use nullherz_conductor::Conductor;
use nullherz_conductor::mixer_orchestrator::MixerOrchestrator;
use nullherz_traits::{Command, DeckParamType, MixerCommand, PerformanceCommand};

fn console() -> Conductor {
    let mut c = Conductor::with_library_path(":memory:");
    c.setup_engine();
    c.bootstrap_4channel_mixer();
    c
}

/// The rate must reach the SAMPLER, which resamples — not the pitch slot, which
/// is the vocoder.
///
/// Sampler parameter 1 is `playback_rate`. Routing this to the vocoder instead
/// would sound approximately plausible and be wrong in every way that matters:
/// -17.8 dB instead of -132 dB, 21.3 ms of added latency, and the tempo would not
/// change at all.
#[test]
fn pitch_reaches_the_sampler_as_playback_rate() {
    let conductor = console();
    let sampler_id = conductor.mixer_manager.deck_mappings[&'A'].sampler_id;

    let out = MixerOrchestrator::translate_command(
        &Command::Mixer(MixerCommand::SetDeckParam {
            deck_id: 'A',
            param_type: DeckParamType::Pitch,
            value: 1.08,
        }),
        &conductor.mixer_manager,
        &conductor.library,
    );

    let hit = out.iter().find_map(|c| match c {
        Command::Mixer(MixerCommand::SetParam { target_id, param_id, value, ramp_duration_samples })
            if *target_id == sampler_id as u64 => Some((*param_id, *value, *ramp_duration_samples)),
        _ => None,
    });

    let (param_id, value, ramp) =
        hit.expect("a Pitch command must produce a SetParam on the deck's own sampler");
    assert_eq!(param_id, 1, "sampler param 1 is playback_rate");
    assert!((value - 1.08).abs() < 1e-6, "the rate must pass through unscaled, got {value}");
    // A rate step is an instantaneous frequency jump and clicks; this fader is
    // swept by hand while listening.
    assert!(ramp > 0, "an unramped rate change is a click");
}

/// The regression this file exists for: KEY LOCK on an UNSYNCED deck that the
/// operator pitched by hand must still cancel the pitch.
#[test]
fn key_lock_cancels_a_manual_pitch_on_an_unsynced_deck() {
    let mut conductor = console();
    let pitch_slot = conductor.mixer_manager.deck_mappings[&'A'].pitch_slot_id;

    // +8% by hand, then KEY LOCK. No SYNC anywhere.
    conductor.apply_mixer_commands(vec![
        Command::Mixer(MixerCommand::SetDeckParam {
            deck_id: 'A',
            param_type: DeckParamType::Pitch,
            value: 1.08,
        }),
        Command::Performance(PerformanceCommand::SetDeckKeyLock { deck_id: 'A', enabled: true }),
    ]);

    assert!(
        !conductor.mixer_manager.sync_decks.contains(&'A'),
        "this test is specifically about the UNSYNCED case"
    );
    let recorded = conductor.mixer_manager.deck_pitch.get(&'A').copied().unwrap_or(1.0);
    assert!(
        (recorded - 1.08).abs() < 1e-6,
        "the fader position must be recorded where key lock can read it, got {recorded}"
    );

    // Re-translating the latch now reports the correction it would apply.
    let out = MixerOrchestrator::translate_command(
        &Command::Performance(PerformanceCommand::SetDeckKeyLock { deck_id: 'A', enabled: true }),
        &conductor.mixer_manager,
        &conductor.library,
    );
    let semis = out
        .iter()
        .find_map(|c| match c {
            Command::Mixer(MixerCommand::SetParam { target_id, param_id: 0, value, .. })
                if *target_id == pitch_slot as u64 => Some(*value),
            _ => None,
        })
        .expect("key lock must set the pitch slot's semitone parameter");

    let expected = -12.0 * 1.08f32.log2(); // -1.333 st
    assert!(
        (semis - expected).abs() < 0.01,
        "KEY LOCK on a hand-pitched deck must transpose {expected:.3} st to cancel +8%, got \
         {semis:.3}. Zero means the correction is reading the sync ratio alone and ignoring the fader."
    );
}

/// With the fader centred there is nothing to cancel, so KEY LOCK must be a
/// no-op — not a small nonzero shift that detunes a track nobody asked to move.
#[test]
fn key_lock_is_silent_when_the_fader_is_centred() {
    let mut conductor = console();
    let pitch_slot = conductor.mixer_manager.deck_mappings[&'A'].pitch_slot_id;

    conductor.apply_mixer_commands(vec![Command::Performance(
        PerformanceCommand::SetDeckKeyLock { deck_id: 'A', enabled: true },
    )]);

    let out = MixerOrchestrator::translate_command(
        &Command::Performance(PerformanceCommand::SetDeckKeyLock { deck_id: 'A', enabled: true }),
        &conductor.mixer_manager,
        &conductor.library,
    );
    let semis = out
        .iter()
        .find_map(|c| match c {
            Command::Mixer(MixerCommand::SetParam { target_id, param_id: 0, value, .. })
                if *target_id == pitch_slot as u64 => Some(*value),
            _ => None,
        })
        .expect("the pitch slot parameter is always set");

    assert!(semis.abs() < 1e-4, "an unpitched, unsynced deck needs no correction, got {semis}");
}
