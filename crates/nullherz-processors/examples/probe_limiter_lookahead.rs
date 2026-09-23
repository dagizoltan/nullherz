//! **What does the master limiter's look-ahead actually buy?**
//!
//! It is 2.0 ms by default — 96 samples at 48 kHz — and that is REAL output
//! latency, correctly declared. At the shipped 256-frame period it is noise
//! against 23 ms of total latency. At 32 frames it is 43% of everything, and at
//! 16 frames it is 60%: look-ahead is measured in SAMPLES, so it does not shrink
//! with the block, it just becomes the majority.
//!
//! # Why this is worth measuring rather than reasoning about
//!
//! The textbook reason for a long look-ahead is to RAMP the gain down gently
//! across the window, so the envelope change does not itself distort. This
//! limiter does not do that: `envelope = window_max.max(envelope * release_coef)`
//! is an INSTANT attack. The gain drops in a single sample the moment a peak
//! enters the window, however wide the window is.
//!
//! If that is the whole story, all the look-ahead buys is that the drop happens
//! before the peak emerges from the delay line — and that needs far fewer than
//! 96 samples. Which would mean most of the 2 ms is free latency.
//!
//! It might not be the whole story, which is the point of measuring. A shorter
//! window also holds the gain down for less time around a peak, so sustained
//! material may behave differently from a transient.
//!
//! # What it reports
//!
//!   * **overshoot** — peak output against the ceiling. Anything above 1.0000 is
//!     a brickwall failing at its one job, and is disqualifying at any latency.
//!   * **THD+N on a limited tone** — the distortion the gain envelope adds while
//!     it is actually working.
//!   * **THD+N below threshold** — transparency when the limiter should be doing
//!     nothing at all.
//!
//!   cargo run --release -p nullherz-processors --example probe_limiter_lookahead

use audio_dsp::measurement::{analyser_floor, thd_n, tone_sample};
use nullherz_processors::limiter::LimiterProcessor;
use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor};

const SR: f32 = 48_000.0;
const FFT: usize = 32_768;
const WARM: usize = 8_192;

const P_THRESHOLD: u32 = 0;
const P_RELEASE_MS: u32 = 1;
const P_LOOKAHEAD_MS: u32 = 2;
const P_CEILING: u32 = 3;

fn db(x: f32) -> f32 { 20.0 * x.max(1e-20).log10() }

fn ctx() -> ProcessContext<'static> {
    ProcessContext { transport: None, host: None, sub_block_offset: 0, is_last_sub_block: true }
}

fn make(lookahead_ms: f32, release_ms: f32, threshold: f32) -> LimiterProcessor {
    let mut lim = LimiterProcessor::new(0, SR);
    lim.set_parameter(P_THRESHOLD, threshold, 0);
    lim.set_parameter(P_CEILING, 1.0, 0);
    lim.set_parameter(P_LOOKAHEAD_MS, lookahead_ms, 0);
    lim.set_parameter(P_RELEASE_MS, release_ms, 0);
    lim
}

/// Run a mono signal through, one block at a time.
fn run(lim: &mut LimiterProcessor, input: &[f32]) -> Vec<f32> {
    const BLOCK: usize = 256;
    let mut out = vec![0.0f32; input.len()];
    let mut c = ctx();
    for start in (0..input.len()).step_by(BLOCK) {
        let n = BLOCK.min(input.len() - start);
        let (a, b) = out.split_at_mut(start);
        let _ = a;
        let ins: [&[f32]; 1] = [&input[start..start + n]];
        let mut outs: [&mut [f32]; 1] = [&mut b[..n]];
        lim.process(&ins, &mut outs, &mut c);
    }
    out
}

/// Worst peak out of a limiter fed an over-ceiling impulse at every position in
/// a look-ahead window — the case the window exists for.
fn worst_overshoot(lookahead_ms: f32, release_ms: f32) -> f32 {
    let mut worst = 0.0f32;
    for pos in 2_000..2_000 + 256 {
        let mut lim = make(lookahead_ms, release_ms, 1.0);
        let mut input = vec![0.0f32; 8192];
        input[pos] = 4.0; // +12 dB over ceiling
        let out = run(&mut lim, &input);
        worst = worst.max(out.iter().fold(0.0f32, |a, v| a.max(v.abs())));
    }
    worst
}

/// THD+N of a tone driven `over_db` above the threshold, so the limiter works.
fn thd_limiting(lookahead_ms: f32, release_ms: f32, freq: f32, over_db: f32) -> f32 {
    let amp = 0.5 * 10f32.powf(over_db / 20.0);
    let mut lim = make(lookahead_ms, release_ms, 0.5);
    let n = WARM + FFT;
    let input: Vec<f32> = (0..n).map(|i| tone_sample(i, freq, SR, amp)).collect();
    let out = run(&mut lim, &input);
    db(thd_n(&out[WARM..], freq, SR, FFT))
}

fn main() {
    println!("Limiter look-ahead sweep — {SR} Hz, ceiling 1.0.");
    println!("Analyser floor {:.1} dB.\n", db(analyser_floor(997.0, SR, FFT)));
    println!("Look-ahead is measured in SAMPLES, so its latency cost is flat at every");
    println!("block size — 43% of the total at a 32-frame period, 60% at 16.\n");

    println!("  {:>9} {:>8} {:>11} {:>12} {:>12} {:>12}",
        "lookahead", "samples", "latency", "overshoot", "THD 997Hz", "THD 60Hz");
    println!("  {:>9} {:>8} {:>11} {:>12} {:>12} {:>12}",
        "", "", "", "(+12dB hit)", "(+6dB over)", "(+6dB over)");

    for la_ms in [2.0f32, 1.0, 0.5, 0.25, 0.1] {
        let samples = (la_ms * 0.001 * SR).round() as usize;
        let over = worst_overshoot(la_ms, 1.0); // fastest release = hardest case
        let t997 = thd_limiting(la_ms, 100.0, 997.0, 6.0);
        let t60 = thd_limiting(la_ms, 100.0, 60.0, 6.0);
        let flag = if over > 1.0001 { "  <- OVERSHOOT" } else { "" };
        println!("  {:>7.2}ms {:>8} {:>9.2}ms {:>12.4} {:>9.1} dB {:>9.1} dB{}",
            la_ms, samples, la_ms, over, t997, t60, flag);
    }

    println!("\n  transparency below threshold (limiter should be doing NOTHING):");
    println!("  {:>9} {:>14} {:>14}", "lookahead", "THD 997Hz", "THD 60Hz");
    for la_ms in [2.0f32, 0.5, 0.1] {
        // -6 dB into a 0.5 threshold: never engages.
        let t997 = thd_limiting(la_ms, 100.0, 997.0, -6.0);
        let t60 = thd_limiting(la_ms, 100.0, 60.0, -6.0);
        println!("  {:>7.2}ms {:>11.1} dB {:>11.1} dB", la_ms, t997, t60);
    }

    println!("\nReading it: overshoot above 1.0000 disqualifies a setting whatever it");
    println!("saves — a brickwall that passes the transient it exists to catch is not");
    println!("one. Low frequency is the telling THD column: one cycle of 60 Hz is");
    println!("16.7 ms, so a gain change inside it distorts the waveform rather than");
    println!("merely riding its level.");
}
