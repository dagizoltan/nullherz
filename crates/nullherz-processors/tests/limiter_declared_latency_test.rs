//! The limiter's declared latency must be its actual delay.
//!
//! PDC compensates every path by what `latency_samples()` reports, so a wrong
//! declaration misaligns decks by exactly its error — and it is invisible while
//! every deck carries the same chain, which is the condition `pdc_latency_test`
//! describes. That test pins the plumbing: the registry reports the value, the
//! compiled plan carries it, engaging KeySync adds it. None of it checks the
//! number is true.
//!
//! The limiter is the one declaration that can be checked exactly. Its latency
//! is a lookahead — a pure delay — so an impulse's onset IS the latency, with no
//! interpretation needed.
//!
//! The FFT-based declarations deliberately are NOT asserted here. An overlap-add
//! pipeline synthesises through a rising window, so an impulse onset lands past
//! the algorithmic latency (`probe_declared_latency` measures ~1187 against a
//! declared 1024 for `PersonalityInheritance`, and that 163-sample gap is the
//! window fade-in, not a defect). With `hop_size = fft.size / 2` the algorithmic
//! latency is the window length, so `fft.size` is correct and conventional.
//! Asserting it from an impulse would be asserting the measurement's bias.

use nullherz_processors::limiter::LimiterProcessor;
use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor, Transport};

const BLOCK: usize = 256;
const SAMPLE_RATE: f32 = 48_000.0;

fn transport() -> Transport {
    Transport {
        bpm: 120.0,
        beat_position: 0.0,
        is_playing: true,
        sample_rate: SAMPLE_RATE,
        absolute_samples: 0,
        system_time_ns: 0,
        device_time_ns: 0,
    }
}

/// Sample index of the first output above 5% of the tail's peak.
fn impulse_onset(limiter: &mut LimiterProcessor, blocks: usize) -> Option<usize> {
    let t = transport();
    let mut tail: Vec<f32> = Vec::with_capacity(BLOCK * blocks);
    for b in 0..blocks {
        let mut inp = [0.0f32; BLOCK];
        if b == 0 {
            // Well below threshold, so the limiter passes it as a pure delay
            // rather than attenuating the thing being measured.
            inp[0] = 0.2;
        }
        let mut out = [0.0f32; BLOCK];
        {
            let ins: [&[f32]; 1] = [&inp];
            let o = &mut out[..];
            let mut outs: [&mut [f32]; 1] = [o];
            let mut ctx = ProcessContext {
                transport: Some(&t),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };
            limiter.process(&ins, &mut outs, &mut ctx);
        }
        tail.extend(out.iter().map(|v| v.abs()));
    }
    let peak = tail.iter().fold(0.0f32, |m, v| m.max(*v));
    if peak <= 1e-6 {
        return None;
    }
    tail.iter().position(|v| *v > peak * 0.05)
}

#[test]
fn test_the_limiter_delays_by_exactly_what_it_declares() {
    let mut limiter = LimiterProcessor::new(0, SAMPLE_RATE);
    limiter.setup(nullherz_traits::AudioConfig { sample_rate: SAMPLE_RATE, block_size: BLOCK });

    let declared = limiter.latency_samples();
    assert!(
        declared > 0,
        "precondition: the limiter should declare its lookahead, got {declared}"
    );

    let onset = impulse_onset(&mut limiter, 32).expect("limiter produced no output");
    assert_eq!(
        onset, declared,
        "the limiter declares {declared} samples of latency but delayed an impulse by {onset}. \
         PDC compensates by the declared number, so the difference is how far this path would \
         be misaligned against a deck that does not carry a limiter."
    );
}

/// And the declaration must follow the lookahead when it changes, or a user
/// adjusting lookahead silently invalidates the compensation.
#[test]
fn test_the_declaration_follows_a_changed_lookahead() {
    let mut limiter = LimiterProcessor::new(0, SAMPLE_RATE);
    limiter.setup(nullherz_traits::AudioConfig { sample_rate: SAMPLE_RATE, block_size: BLOCK });
    let before = limiter.latency_samples();

    // Parameter 2 is Lookahead (ms), declared range 0.1..5.0.
    limiter.set_parameter(2, 4.0, 0);
    let after = limiter.latency_samples();
    assert_ne!(
        before, after,
        "lookahead was changed from its default to 4 ms but the declared latency stayed at \
         {before} samples — PDC would keep compensating by the old value"
    );

    let onset = impulse_onset(&mut limiter, 32).expect("limiter produced no output");
    assert_eq!(
        onset, after,
        "after changing lookahead the limiter declares {after} samples but delayed an impulse \
         by {onset}"
    );
}
