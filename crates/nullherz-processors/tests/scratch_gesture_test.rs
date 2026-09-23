//! Scratching hands the deck's rate to a gesture, and gives it back.
//!
//! Moving the playhead in jumps chops the audio; what makes a scratch sound
//! like a record is that PITCH and DIRECTION follow the hand. So the gesture
//! drives `playback_rate`, not position, and the voice advances by that rate
//! per output sample exactly as it does in normal playback.
//!
//! The half that is easy to get wrong is the release. A scratch has to leave
//! the deck the way it found it: scratching a stopped deck must not leave it
//! running, and scratching a playing one must not stop it.

use nullherz_processors::sampler::SamplerProcessor;
use nullherz_traits::{AudioProcessor, Command, PerformanceCommand, SampleMetadata, SignalProcessor};
use std::sync::Arc;

const FRAMES: usize = 96_000;

fn loaded() -> SamplerProcessor {
    let mut s = SamplerProcessor::new(1);
    let buf: Arc<Vec<f32>> = Arc::new((0..FRAMES).map(|i| (i as f32 * 0.005).sin() * 0.7).collect());
    let mut meta = SampleMetadata::new_empty();
    meta.sample_rate = 48_000;
    meta.channels = 1;
    meta.total_samples = FRAMES as u64;
    s.apply_topology_mutation(nullherz_traits::TopologyMutation::AddSource {
        node_idx: 1,
        buffer: buf,
        sample_id: 3,
        metadata: Some(Arc::new(meta)),
    });
    s
}

fn scratch(s: &mut SamplerProcessor, active: bool, rate: f32) {
    s.apply_command(&Command::Performance(PerformanceCommand::SetScratch {
        node_idx: 1,
        active,
        rate,
    }));
}

fn play(s: &mut SamplerProcessor) {
    s.apply_command(&Command::Performance(PerformanceCommand::PlayNode { node_idx: 1 }));
}

/// Render a block and report peak level, so "is it making a sound" is testable.
fn render(s: &mut SamplerProcessor, n: usize) -> f32 {
    let mut l = vec![0.0f32; n];
    let mut r = vec![0.0f32; n];
    let mut ctx = nullherz_traits::ProcessContext {
        transport: None, host: None, sub_block_offset: 0, is_last_sub_block: true,
    };
    {
        let ins: [&[f32]; 0] = [];
        let (a, b) = (&mut l[..], &mut r[..]);
        let mut outs: [&mut [f32]; 2] = [a, b];
        s.process(&ins, &mut outs, &mut ctx);
    }
    l.iter().chain(r.iter()).fold(0.0f32, |m, v| m.max(v.abs()))
}

fn pos(s: &SamplerProcessor) -> u64 { s.get_playback_position() }

#[test]
fn test_scratching_a_stopped_deck_makes_sound() {
    // The gesture on a parked deck is the signature scratch. A stopped deck has
    // no sounding voice, so this only works if the gesture starts one.
    let mut s = loaded();
    s.apply_command(&Command::Performance(PerformanceCommand::NudgePosition { node_idx: 1, frames: 20_000 }));
    scratch(&mut s, true, 1.0);
    let peak = render(&mut s, 512);
    assert!(
        peak > 0.01,
        "scratching a stopped deck produced silence (peak {peak}) — the gesture did not \
         start a voice, so there is nothing to hear"
    );
}

#[test]
fn test_scratch_rate_drives_the_playhead_speed() {
    // Double rate must cover twice the ground in the same number of samples.
    let mut a = loaded();
    a.apply_command(&Command::Performance(PerformanceCommand::NudgePosition { node_idx: 1, frames: 10_000 }));
    scratch(&mut a, true, 1.0);
    let start_a = pos(&a);
    render(&mut a, 512);
    let moved_1x = pos(&a) - start_a;

    let mut b = loaded();
    b.apply_command(&Command::Performance(PerformanceCommand::NudgePosition { node_idx: 1, frames: 10_000 }));
    scratch(&mut b, true, 2.0);
    let start_b = pos(&b);
    render(&mut b, 512);
    let moved_2x = pos(&b) - start_b;

    assert!(
        moved_2x > moved_1x + 200,
        "rate 2.0 moved {moved_2x} frames vs {moved_1x} at rate 1.0 — the gesture's rate \
         is not reaching the voice"
    );
}

#[test]
fn test_negative_rate_pulls_the_track_backwards() {
    let mut s = loaded();
    s.apply_command(&Command::Performance(PerformanceCommand::NudgePosition { node_idx: 1, frames: 40_000 }));
    scratch(&mut s, true, -1.0);
    let before = pos(&s);
    let peak = render(&mut s, 512);
    let after = pos(&s);

    assert!(after < before, "backwards scratch went from {before} to {after}");
    assert!(peak > 0.01, "backwards scratch is silent (peak {peak}) — reverse must be audible");
}

#[test]
fn test_releasing_leaves_a_stopped_deck_stopped() {
    // The restore contract. Getting this wrong means a deck silently starts
    // playing out of the mix because someone nudged its waveform.
    let mut s = loaded();
    s.apply_command(&Command::Performance(PerformanceCommand::NudgePosition { node_idx: 1, frames: 5_000 }));
    scratch(&mut s, true, 1.0);
    render(&mut s, 256);
    scratch(&mut s, false, 0.0);

    let peak = render(&mut s, 512);
    assert!(
        peak < 1.0e-6,
        "deck is still sounding (peak {peak}) after a scratch was released on a STOPPED deck"
    );
}

#[test]
fn test_releasing_leaves_a_playing_deck_playing() {
    let mut s = loaded();
    play(&mut s);
    render(&mut s, 256);
    scratch(&mut s, true, -2.0);
    render(&mut s, 256);
    scratch(&mut s, false, 0.0);

    let before = pos(&s);
    let peak = render(&mut s, 512);
    assert!(peak > 0.01, "deck went silent (peak {peak}) after releasing a scratch while playing");
    assert!(pos(&s) > before, "deck did not resume forward playback after the scratch");
}

#[test]
fn test_release_keeps_the_position_the_gesture_left_it_at() {
    // Letting go must not snap back to where the scratch started.
    let mut s = loaded();
    s.apply_command(&Command::Performance(PerformanceCommand::NudgePosition { node_idx: 1, frames: 30_000 }));
    scratch(&mut s, true, 2.0);
    render(&mut s, 1024);
    let at_release = pos(&s);
    scratch(&mut s, false, 0.0);

    let after = pos(&s);
    assert!(
        after.abs_diff(at_release) < 64,
        "position jumped from {at_release} to {after} on release"
    );
}

#[test]
fn test_scratch_ignores_other_decks() {
    let mut s = loaded();
    play(&mut s);
    render(&mut s, 256);
    let before = pos(&s);
    s.apply_command(&Command::Performance(PerformanceCommand::SetScratch {
        node_idx: 99, active: true, rate: -8.0,
    }));
    render(&mut s, 256);
    assert!(
        pos(&s) > before,
        "a scratch addressed to node 99 hijacked node 1 — dragging one deck would drag them all"
    );
}

/// The pitch fader and a scratch gesture must not fight over the voice rate.
///
/// Both write `playback_rate`: the fader through parameter 1, the gesture
/// directly. A gesture OWNS that rate while it is held — the voice plays at the
/// speed of a hand — and the deck's own rate is what the release restores to.
///
/// If parameter 1 wrote voices mid-gesture it would clobber the hand's rate for a
/// frame and then self-correct on release. That presents as "scratching sometimes
/// stutters", is inaudible in a test that only checks the endpoints, and never
/// reproduces on demand. Measured through the playhead, which is what a listener
/// hears.
#[test]
fn test_pitch_fader_does_not_disturb_a_held_scratch() {
    let mut s = loaded();
    // Start well inside the buffer so a backwards gesture has room.
    s.apply_command(&Command::Performance(PerformanceCommand::NudgePosition { node_idx: 1, frames: 60_000 }));
    // +8% on the fader before any gesture.
    s.set_parameter(1, 1.08);
    play(&mut s);

    // Grab it: the hand takes the rate, pulling backwards at 2x.
    scratch(&mut s, true, -2.0);
    let a = pos(&s);
    render(&mut s, 512);
    let b = pos(&s);
    let held_speed = a as f64 - b as f64; // backwards, so a > b
    assert!(
        held_speed > 512.0 * 1.5,
        "a -2.0 gesture should pull the playhead back ~1024 frames over 512 samples, moved \
         {held_speed}"
    );

    // Nudge the fader mid-gesture. The gesture must be unaffected.
    s.set_parameter(1, 0.96);
    let c = pos(&s);
    render(&mut s, 512);
    let d = pos(&s);
    let after_nudge = c as f64 - d as f64;
    assert!(
        after_nudge > 512.0 * 1.5,
        "moving the pitch fader mid-scratch must not take the rate away from the hand: the \
         playhead moved {after_nudge} frames over 512 samples, which is the fader's rate, not \
         the gesture's"
    );

    // Let go: the deck plays forward at the fader's NEW position.
    scratch(&mut s, false, 0.0);
    let e = pos(&s);
    render(&mut s, 512);
    let f = pos(&s);
    let released = f as f64 - e as f64;
    assert!(
        released > 0.0,
        "release must resume forward playback, moved {released}"
    );
    assert!(
        (released - 512.0 * 0.96).abs() < 512.0 * 0.15,
        "release must resume at the fader's rate (0.96 -> ~492 frames per 512 samples), got \
         {released}. 512 would mean the fader was ignored and the deck reset to 1.0."
    );
}
