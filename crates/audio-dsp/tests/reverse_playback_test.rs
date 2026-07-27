//! A voice must be able to play backwards, and forwards must not change.
//!
//! Scratching and scrubbing move the playhead in both directions. The render
//! loops only ever checked the END of the buffer, which was sufficient while
//! the rate was positive. It is not a bounds violation when the rate goes
//! negative — `play_head.floor() as usize` saturates a negative value to 0 —
//! but that is worse than a panic: the voice keeps sampling index 0 while
//! handing the interpolator a negative fraction, so it extrapolates and emits
//! plausible-looking garbage instead of audio.
//!
//! The forward assertions matter as much as the reverse ones. This is the RT
//! hot path, so the change had to be inert for every existing caller.

use audio_dsp::oscillators::SamplerVoice;
use std::sync::Arc;

const FRAMES: usize = 2_048;

/// A ramp, so a sample's value identifies the position it came from.
fn ramp() -> Arc<Vec<f32>> {
    Arc::new((0..FRAMES).map(|i| i as f32 / FRAMES as f32).collect())
}

fn voice_at(pos: f64, rate: f32) -> SamplerVoice {
    let mut v = SamplerVoice::new();
    v.trigger(ramp(), rate, 1.0);
    v.play_head = pos;
    v.buffer_frames = FRAMES;
    v.buffer_channels = 1;
    v
}

#[test]
fn test_forward_playback_is_unchanged() {
    // The regression guard. If this moves, the start-boundary check altered the
    // ordinary path and every deck in the console sounds different.
    let mut v = voice_at(0.0, 1.0);
    let mut out = vec![0.0f32; 256];
    v.process_block(&mut out);

    for (i, s) in out.iter().enumerate().take(200) {
        let expected = i as f32 / FRAMES as f32;
        assert!(
            (s - expected).abs() < 1e-3,
            "forward sample {i} is {s}, expected ~{expected}"
        );
    }
    assert!(v.play_head > 199.0, "playhead did not advance: {}", v.play_head);
}

#[test]
fn test_reverse_playback_walks_the_buffer_backwards() {
    let start = 1_000.0;
    let mut v = voice_at(start, -1.0);
    let mut out = vec![0.0f32; 256];
    v.process_block(&mut out);

    assert!(
        v.play_head < start,
        "playhead did not move backwards: {} (started at {start})",
        v.play_head
    );

    // Values must DESCEND, because the buffer is a rising ramp read in reverse.
    for (i, s) in out.iter().enumerate().take(200) {
        let expected = (start as f32 - i as f32) / FRAMES as f32;
        assert!(
            (s - expected).abs() < 1e-3,
            "reverse sample {i} is {s}, expected ~{expected} — the voice is not \
             reading the buffer backwards"
        );
    }
}

#[test]
fn test_reverse_output_is_the_forward_output_mirrored() {
    // Stronger than "descending": the audio played backwards from a point must
    // be the same material as the audio played forwards into it.
    let mut fwd = voice_at(500.0, 1.0);
    let mut a = vec![0.0f32; 128];
    fwd.process_block(&mut a);

    let mut rev = voice_at(500.0 + 127.0, -1.0);
    let mut b = vec![0.0f32; 128];
    rev.process_block(&mut b);

    for i in 0..120 {
        let mirrored = b[127 - i];
        assert!(
            (a[i] - mirrored).abs() < 2e-3,
            "forward[{i}]={} does not match reverse[{}]={mirrored}",
            a[i],
            127 - i
        );
    }
}

#[test]
fn test_reverse_holds_at_the_start_instead_of_extrapolating() {
    // Scrub a voice off the front of the buffer. It must clamp at 0 and stop,
    // not produce samples from a negative fraction.
    let mut v = voice_at(4.0, -1.0);
    let mut out = vec![0.0f32; 256];
    v.process_block(&mut out);

    assert!(
        v.play_head >= 0.0,
        "playhead went negative: {} — the interpolator would extrapolate",
        v.play_head
    );
    assert!(
        v.play_head < 1.0,
        "playhead should be pinned at the buffer start, got {}",
        v.play_head
    );
    // Everything after the boundary is silence, not extrapolated noise.
    for (i, s) in out.iter().enumerate().skip(10) {
        assert_eq!(*s, 0.0, "sample {i} after the start boundary is {s}, expected silence");
    }
}

#[test]
fn test_a_voice_scrubbed_to_the_start_can_play_forward_again() {
    // Clamping must not kill the voice. Pushing a record back to its lead-in and
    // then letting go has to resume.
    let mut v = voice_at(3.0, -1.0);
    let mut out = vec![0.0f32; 64];
    v.process_block(&mut out);
    assert!(v.is_active, "voice deactivated when it hit the start of the buffer");

    v.playback_rate = 1.0;
    let mut out2 = vec![0.0f32; 64];
    v.process_block(&mut out2);
    assert!(
        v.play_head > 1.0,
        "voice did not resume forward playback after being scrubbed to the start"
    );
    assert!(out2.iter().any(|s| *s != 0.0), "no audio after resuming");
}

#[test]
fn test_forward_still_deactivates_at_the_end() {
    // The end boundary must be untouched by the start-boundary work.
    let mut v = voice_at(FRAMES as f64 - 10.0, 1.0);
    let mut out = vec![0.0f32; 256];
    v.process_block(&mut out);
    assert!(!v.is_active, "voice ran past the end of its buffer without deactivating");
}
