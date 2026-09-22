//! The master limiter is a BRICKWALL: nothing may leave it above the ceiling.
//!
//! It used to, by exactly one release step. The look-ahead window was
//! `[t - lookahead + 1, t]` while the delay line emitted the sample at
//! `t - lookahead`, so the gain applied to a peak was the envelope as it stood
//! one sample later — already decayed. The overshoot is `exp(1/release_samples)`:
//! +0.002 dB at the 100 ms default, and +0.20 dB at the fastest release the
//! parameter clamp allows (1 ms). Small in dB, but a brickwall that passes the
//! transient it exists to catch is not a brickwall.
//!
//! The fast-release case is the one that matters and the one nothing covered:
//! `examples/bench_console_block.rs` printed `output peak 1.0002` the whole
//! time, into an assertion that only checked the value was greater than zero.

use nullherz_processors::limiter::LimiterProcessor;
use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor};

const SR: f32 = 44_100.0;

/// Param ids, from `LimiterProcessor::set_parameter`.
const P_THRESHOLD: u32 = 0;
const P_RELEASE_MS: u32 = 1;
const P_LOOKAHEAD_MS: u32 = 2;
const P_CEILING: u32 = 3;

fn ctx() -> ProcessContext<'static> {
    ProcessContext { transport: None, host: None, sub_block_offset: 0, is_last_sub_block: true }
}

/// Peak out of a limiter fed silence with one over-ceiling impulse at `pos`.
fn peak_with_impulse_at(pos: usize, release_ms: f32, amplitude: f32) -> f32 {
    let mut lim = LimiterProcessor::new(0, SR);
    lim.set_parameter(P_THRESHOLD, 1.0, 0);
    lim.set_parameter(P_CEILING, 1.0, 0);
    lim.set_parameter(P_LOOKAHEAD_MS, 2.0, 0);
    lim.set_parameter(P_RELEASE_MS, release_ms, 0);

    let n = 8192;
    let mut input = vec![0.0f32; n];
    input[pos] = amplitude;
    let mut out = vec![0.0f32; n];

    let ins: [&[f32]; 1] = [&input];
    let mut outs: [&mut [f32]; 1] = [&mut out];
    lim.process(&ins, &mut outs, &mut ctx());

    out.iter().fold(0.0f32, |a, &v| a.max(v.abs()))
}

/// Walk the impulse across a full look-ahead window at each release setting.
/// A misalignment of even one sample shows up at some phase.
#[test]
fn nothing_escapes_the_ceiling_at_any_release() {
    // 2 ms of look-ahead at 44.1 kHz is 88 samples; sweep well past it.
    const START: usize = 256;
    const SPAN: usize = 300;

    // release_ms clamps to [1, 1000]; 1 ms is the fastest reachable and the
    // worst case for the alignment error.
    for release_ms in [1000.0f32, 100.0, 10.0, 1.0] {
        let mut worst = 0.0f32;
        let mut worst_pos = 0usize;
        for pos in START..START + SPAN {
            let peak = peak_with_impulse_at(pos, release_ms, 4.0); // +12 dB over
            if peak > worst {
                worst = peak;
                worst_pos = pos;
            }
        }
        assert!(
            worst <= 1.0 + 1e-4,
            "release {release_ms} ms: a 12 dB overshoot left the brickwall at {worst:.6} \
             (ceiling 1.0, impulse at sample {worst_pos}). Overshoot of exp(1/release_samples) \
             means the look-ahead window excludes the sample being emitted."
        );
    }
}

/// A sustained over-ceiling signal must be held at the ceiling, not just
/// attenuated somewhere near it — this is the steady-state half of the above.
#[test]
fn sustained_overload_is_held_at_the_ceiling() {
    let mut lim = LimiterProcessor::new(0, SR);
    lim.set_parameter(P_THRESHOLD, 1.0, 0);
    lim.set_parameter(P_CEILING, 1.0, 0);
    lim.set_parameter(P_LOOKAHEAD_MS, 2.0, 0);
    lim.set_parameter(P_RELEASE_MS, 1.0, 0);

    let n = 8192;
    let input = vec![2.0f32; n];
    let mut out = vec![0.0f32; n];
    let ins: [&[f32]; 1] = [&input];
    let mut outs: [&mut [f32]; 1] = [&mut out];
    lim.process(&ins, &mut outs, &mut ctx());

    // Skip the look-ahead fill at the head.
    let settled = &out[512..];
    let peak = settled.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
    assert!(peak <= 1.0 + 1e-4, "sustained overload reached {peak:.6} above a ceiling of 1.0");
    assert!(peak > 0.9, "limiter over-attenuated a sustained overload to {peak:.6}");
}

/// Below the threshold the limiter is transparent — the off-by-one fix must
/// not have introduced a gain change on material that needs no limiting.
#[test]
fn quiet_material_passes_unchanged() {
    let mut lim = LimiterProcessor::new(0, SR);
    lim.set_parameter(P_THRESHOLD, 1.0, 0);
    lim.set_parameter(P_CEILING, 1.0, 0);
    lim.set_parameter(P_LOOKAHEAD_MS, 2.0, 0);

    let n = 4096;
    let input: Vec<f32> = (0..n)
        .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / SR).sin() * 0.25)
        .collect();
    let mut out = vec![0.0f32; n];
    let ins: [&[f32]; 1] = [&input];
    let mut outs: [&mut [f32]; 1] = [&mut out];
    lim.process(&ins, &mut outs, &mut ctx());

    let lookahead = lim.latency_samples();
    assert!(lookahead > 0, "the look-ahead delay is the limiter's declared latency");
    for i in lookahead..n {
        let expected = input[i - lookahead];
        assert!(
            (out[i] - expected).abs() < 1e-6,
            "sample {i}: expected {expected:.6} (input delayed by {lookahead}), got {:.6}",
            out[i]
        );
    }
}
