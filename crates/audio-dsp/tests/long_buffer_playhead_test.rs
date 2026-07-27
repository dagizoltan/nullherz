//! A sampler voice must keep advancing, and keep interpolating, over buffers
//! longer than f32 can address.
//!
//! `play_head` was f32. f32 holds integers exactly only to 2^24; past
//! 2^26 = 67,108,864 the spacing between representable values is 8, so
//! `play_head += 1.0` rounds straight back to the same value and **playback
//! freezes** — at 25.4 minutes of 44.1 kHz audio, and at 5.8 minutes of
//! 192 kHz. Well before the freeze the fractional part is quantised away
//! (spacing 0.5 from 2^23, 0.25 from 2^22), so the interpolator degrades to
//! sample-and-hold and the requested playback rate stops being honoured.
//!
//! These tests allocate real buffers past those thresholds, so they are
//! memory-hungry by design: the smallest is 2^26 + slack frames of f32
//! (~268 MB). That is the point — nothing smaller can catch the bug.

use audio_dsp::SamplerVoice;
use std::sync::Arc;

/// One past the f32 integer-spacing-8 threshold.
const FREEZE_THRESHOLD: usize = 1 << 26; // 67,108,864 frames

/// Guards the PREMISE of the tests below, and of `SamplerVoice`'s f64
/// accumulators: an f32 position at this magnitude cannot be advanced at all.
///
/// Keep this test. If someone narrows `play_head` back to f32 for "performance",
/// the tests below will fail — but this one explains *why* in two lines, without
/// needing a 268 MB allocation to demonstrate it.
#[test]
fn test_f32_cannot_advance_a_playhead_at_this_magnitude() {
    let base = FREEZE_THRESHOLD as f32;

    assert_eq!(
        base + 1.0,
        base,
        "f32 at 2^26 should be unable to represent +1.0 — if this now passes, \
         the freeze threshold has moved and the comment above needs revisiting"
    );

    // And it does not creep: repeated sub-frame increments never accumulate.
    let mut x = base;
    for _ in 0..100 {
        x += 0.5;
    }
    assert_eq!(x, base, "f32 accumulated {} frames over 100 increments", x - base);

    // f64 has no such trouble at this magnitude.
    let base64 = FREEZE_THRESHOLD as f64;
    assert_eq!(base64 + 1.0 - base64, 1.0);
}

#[test]
fn test_playhead_advances_past_f32_freeze_threshold() {
    // Start just below the threshold so the test allocates as little as it can
    // while still crossing it.
    let start = (FREEZE_THRESHOLD - 512) as f64;
    let frames = FREEZE_THRESHOLD + 4096;

    let buffer = Arc::new(vec![0.25f32; frames]);
    let mut voice = SamplerVoice::new();
    voice.trigger_at(buffer, 1.0, 1.0, start, 0.0);
    voice.set_layout(frames, 1);

    let mut out = vec![0.0f32; 2048];
    let mut outs: Vec<&mut [f32]> = vec![&mut out];
    voice.process_block_planar(&mut outs, 2048);

    let advanced = voice.play_head - start;
    assert!(
        voice.is_active,
        "voice deactivated while still well inside the buffer"
    );
    assert!(
        (advanced - 2048.0).abs() < 1.0,
        "playhead advanced {advanced} frames instead of 2048 across the f32 \
         freeze threshold — position accumulator has lost resolution"
    );
}

#[test]
fn test_fractional_rate_is_honoured_on_a_long_buffer() {
    // A non-integer rate is the sharpest probe: with f32 at this magnitude the
    // increment rounds to a whole frame (or to zero), so the voice plays at the
    // wrong pitch or stops entirely.
    let start = (FREEZE_THRESHOLD - 512) as f64;
    let frames = FREEZE_THRESHOLD + 4096;
    let rate = 0.5f32;

    let buffer = Arc::new(vec![0.25f32; frames]);
    let mut voice = SamplerVoice::new();
    voice.trigger_at(buffer, rate, 1.0, start, 0.0);
    voice.set_layout(frames, 1);

    let blocks = 1024;
    let mut out = vec![0.0f32; blocks];
    let mut outs: Vec<&mut [f32]> = vec![&mut out];
    voice.process_block_planar(&mut outs, blocks);

    let advanced = voice.play_head - start;
    let expected = blocks as f64 * rate as f64;
    assert!(
        (advanced - expected).abs() < 1.0,
        "at rate {rate} the playhead advanced {advanced} frames over {blocks} \
         frames of output; expected {expected}"
    );
}

#[test]
fn test_interpolation_fraction_survives_on_a_long_buffer() {
    // A ramp makes the interpolated value a direct readout of the fractional
    // position. At a .5 offset the output must sit halfway between neighbours;
    // if the fraction has been quantised away it lands exactly on a sample.
    let frames = FREEZE_THRESHOLD + 4096;
    let mut data = vec![0.0f32; frames];
    // Only the window we actually read needs to be a ramp.
    let base = FREEZE_THRESHOLD - 8;
    for i in 0..32 {
        data[base + i] = i as f32;
    }

    let buffer = Arc::new(data);
    let mut voice = SamplerVoice::new();
    voice.interpolation = audio_dsp::InterpolationType::Linear;
    voice.trigger_at(buffer, 1.0, 1.0, (base + 4) as f64 + 0.5, 0.0);
    voice.set_layout(frames, 1);

    let mut out = vec![0.0f32; 1];
    let mut outs: Vec<&mut [f32]> = vec![&mut out];
    voice.process_block_planar(&mut outs, 1);

    // Ramp values at base+4 and base+5 are 4.0 and 5.0; halfway is 4.5.
    assert!(
        (out[0] - 4.5).abs() < 1e-3,
        "interpolated {} at a .5 offset past 2^26 frames; expected 4.5 — the \
         fractional part of the position was lost",
        out[0]
    );
}
