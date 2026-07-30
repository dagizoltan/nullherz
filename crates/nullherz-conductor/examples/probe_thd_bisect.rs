//! **Which node makes the distortion?**
//!
//! `probe_signal_quality` grades the whole console. This one drives candidate
//! nodes STANDALONE on the same ruler (`audio_dsp::measurement`), so a residue
//! can be attributed rather than guessed at.
//!
//! It exists because a single THD number is not a diagnosis, and this console's
//! 0.046% needed three things a scalar cannot give you:
//!
//!   * **an input control** — the same analysis window on the UNPROCESSED
//!     input. Not "a pure sine from sample 0": the very samples that were fed
//!     in. This is what proved the residue was already in the stimulus.
//!   * **a warm-up sweep** — a number that CHANGES with warm-up length is
//!     measuring position, not steady state.
//!   * **a block-size sweep** — a per-block artefact (a re-triggering ramp, a
//!     state reset) tracks block size; a steady-state one does not.
//!
//!   cargo run --release -p nullherz-conductor --example probe_thd_bisect

use audio_dsp::measurement::{thd_n, tone};
use audio_dsp::{DspKernel, Filter};

const SR: f32 = 48_000.0;
const FFT: usize = 16_384;
const TONE: f32 = 997.0;
const AMP: f32 = 0.5;

/// Level at the fundamental in dB, for separating "wrong level" (an LTI gain
/// error) from "wrong waveform" (energy that was not there before).
///
/// Uses the shared analyser's window via `measurement::spectrum`, NOT a
/// hand-rolled one. This function used to build its own Hann-windowed FFT while
/// `thd_n` used Blackman-Harris, so the gain column and the THD column on the
/// same row came off two different instruments.
fn fundamental_db(x: &[f32]) -> f32 {
    let mags = audio_dsp::measurement::spectrum(x, FFT);
    audio_dsp::measurement::level_db_at(&mags, TONE, SR, FFT)
}

/// Run `f` over `warmup + FFT` samples in `block`-sized chunks, discard the
/// warm-up, analyse the tail — against the same window of the raw input.
fn measure(name: &str, warmup: usize, block: usize, mut f: impl FnMut(&[f32], &mut [f32])) {
    let total = warmup + FFT;
    let input = tone(0, total, TONE, SR, AMP);
    let mut output = vec![0.0f32; total];
    let mut i = 0;
    while i < total {
        let n = block.min(total - i);
        f(&input[i..i + n], &mut output[i..i + n]);
        i += n;
    }

    let out_thd = thd_n(&output[warmup..], TONE, SR, FFT);
    let in_thd = thd_n(&input[warmup..], TONE, SR, FFT);
    let gain = fundamental_db(&output[warmup..]) - fundamental_db(&input[warmup..]);
    println!(
        "  {name:<34} THD+N {:>9.7}%  (input {:>9.7}%)   gain {:+.3} dB",
        out_thd * 100.0, in_thd * 100.0, gain
    );
}

fn main() {
    println!("=== CONTROL (identity: in == out) ===");
    measure("passthrough", 4096, 256, |i, o| o.copy_from_slice(i));

    println!("\n=== single nodes at unity, 256-sample blocks ===");

    let mut iso = audio_dsp::DjIsolatorStereo::with_sample_rate(SR);
    measure("DjIsolatorStereo @ unity", 4096, 256, |i, o| {
        let mut r = vec![0.0f32; i.len()];
        iso.process_stereo(i, i, o, &mut r);
    });

    let mut eq = audio_dsp::MasteringEq::with_sample_rate(SR);
    measure("MasteringEq @ unity", 4096, 256, |i, o| {
        let mut oo = [o];
        eq.process(&[i], &mut oo);
    });

    let mut lim = nullherz_processors::LimiterProcessor::new(1, SR);
    measure("Limiter @ unity", 4096, 256, |i, o| {
        let mut ctx = nullherz_traits::ProcessContext {
            transport: None, host: None, sub_block_offset: 0, is_last_sub_block: false };
        let mut oo = [o];
        nullherz_traits::SignalProcessor::process(&mut lim, &[i], &mut oo, &mut ctx);
    });

    let mut gain = audio_dsp::Gain::new(1.0, 0.05);
    measure("Gain @ 1.0", 4096, 256, |i, o| {
        let mut oo = [o];
        gain.process(&[i], &mut oo);
    });

    // ---- the isolator's band re-sum, opened up -----------------------------
    // Low + Mid + High is meant to reconstruct the input. An LR pair sums to
    // ALLPASS (flat magnitude, rotated phase), so this cannot create new
    // frequencies — but it can lose level, and it does. The bands are measured
    // separately so the re-sum error is arithmetic on the page, not inference.
    println!("\n=== DjIsolator band structure at {TONE} Hz ===");
    {
        let n = 4096 + FFT;
        let input = tone(0, n, TONE, SR, AMP);
        let reference = fundamental_db(&input[4096..]);
        let band = |coeffs: Vec<audio_dsp::BiquadCoefficients>, label: &str| {
            let mut fs: Vec<audio_dsp::BiquadFilter> =
                coeffs.iter().map(|c| audio_dsp::BiquadFilter::new(*c)).collect();
            let out: Vec<f32> = input.iter().map(|&x| {
                let mut s = x;
                for f in fs.iter_mut() { s = f.process_sample(s); }
                s
            }).collect();
            println!("  {label:<28} {:+.3} dB", fundamental_db(&out[4096..]) - reference);
        };
        let lp = |hz| audio_dsp::BiquadCoefficients::linkwitz_riley_lp(hz, SR);
        let hp = |hz| audio_dsp::BiquadCoefficients::linkwitz_riley_hp(hz, SR);
        band(vec![lp(300.0); 2], "LOW  = LP300^2");
        band(vec![hp(300.0), hp(300.0), lp(3000.0), lp(3000.0)], "MID  = HP300^2 LP3000^2");
        band(vec![hp(3000.0); 2], "HIGH = HP3000^2");
        println!("  (summed, they are -0.065 dB at {TONE} Hz — see ARCHITECTURE.md)");
    }

    // ---- warm-up sensitivity ----------------------------------------------
    // THE TEST THAT CRACKED IT. A steady-state property does not depend on how
    // long you warmed up. When this column MOVES, whatever is moving is a
    // function of sample POSITION — and the `input` column says whether it is
    // the node or the stimulus. It was the stimulus.
    println!("\n=== warm-up sensitivity (DjIsolatorStereo) ===");
    for w in [0usize, 4096, 16_384, 65_536] {
        let mut iso = audio_dsp::DjIsolatorStereo::with_sample_rate(SR);
        measure(&format!("warmup {w:>6} samples"), w, 256, |i, o| {
            let mut r = vec![0.0f32; i.len()];
            iso.process_stereo(i, i, o, &mut r);
        });
    }

    // ---- the generator, with no processor at all ---------------------------
    println!("\n=== the SIGNAL GENERATOR alone (no processor) ===");
    for off in [0usize, 16_384, 65_536, 131_072] {
        let good = thd_n(&tone(off, FFT, TONE, SR, AMP), TONE, SR, FFT);
        // The pre-fix spelling, kept here as the counter-example: the argument
        // grows with `i`, so an f32 ULP does too.
        let naive: Vec<f32> = (off..off + FFT)
            .map(|i| (i as f32 * TONE * std::f32::consts::TAU / SR).sin() * AMP)
            .collect();
        println!(
            "  from sample {off:>7}:  wrapped f64 phase {:>9.7}%    naive f32 phase {:>9.7}%",
            good * 100.0, thd_n(&naive, TONE, SR, FFT) * 100.0
        );
    }

    // ---- block-size sensitivity -------------------------------------------
    println!("\n=== block-size sensitivity (DjIsolatorStereo) ===");
    for b in [64usize, 256, 1024, FFT + 4096] {
        let mut iso = audio_dsp::DjIsolatorStereo::with_sample_rate(SR);
        measure(&format!("block {b:>6}"), 4096, b, |i, o| {
            let mut r = vec![0.0f32; i.len()];
            iso.process_stereo(i, i, o, &mut r);
        });
    }
}
