//! **Is the console FLAT?**
//!
//! THD says nothing about frequency response: a filter that is 3 dB down at
//! 300 Hz adds no distortion at all, it just isn't transparent. This is the last
//! unmeasured fidelity axis, and it has a live suspect.
//!
//! `DjIsolator` splits 3 ways as `LOW = LP300^2`, `MID = HP300^2 LP3000^2`,
//! `HIGH = HP3000^2` and sums. A Linkwitz-Riley pair sums to ALLPASS — flat
//! magnitude, rotated phase — but that guarantee holds for a PAIR. Here the
//! high band never passes through the 300 Hz section, so the three bands are not
//! a nested pair of LR splits and nothing guarantees they reconstruct. Measured
//! at 997 Hz the error is -0.065 dB, which says nothing about 300 Hz or 3 kHz
//! where the bands actually cross. This measures there.
//!
//! Also sweeps the CORRECT nesting (`HIGH = HP300^2 HP3000^2`, i.e. split at 300
//! into low/rest and then split *rest* at 3000) so the error is a comparison
//! rather than an assertion.
//!
//! # Measurement notes
//!
//! * FFT is 32_768, not 16_384. The fundamental window is +/-8 bins; at
//!   16_384 that is +/-23 Hz, so a 30 Hz tone's window would reach across DC and
//!   read garbage. At 32_768 it is +/-11.7 Hz and the sweep can start low.
//! * Long warm-up. An LR4 at 300 Hz rings for a while, and measuring the head of
//!   the output reads the transient, not the response.
//!
//!   cargo run --release -p nullherz-conductor --example probe_frequency_response

use std::sync::Arc;

use audio_dsp::measurement::{level_db_at, spectrum, tone_sample};
use audio_dsp::Filter;
use nullherz_traits::{Command, PerformanceCommand};

const SR: f32 = 48_000.0;
const FFT: usize = 32_768;
const AMP: f32 = 0.5;
/// Generous: a 300 Hz LR4 has a long tail, and the head of the output is its
/// transient rather than its steady-state response.
const WARM: usize = 32_768;
const BLOCK: usize = 256;
const SETTLE_BLOCKS: usize = 256;

/// Log-spaced sweep. Dense around the 300 Hz and 3 kHz crossovers, because a
/// re-sum error lives exactly where the bands hand over and a coarse sweep can
/// step straight over it.
fn sweep_points() -> Vec<f32> {
    let mut v: Vec<f32> = Vec::new();
    let (lo, hi, n) = (30.0f32, 20_000.0f32, 48);
    for i in 0..=n {
        v.push(lo * (hi / lo).powf(i as f32 / n as f32));
    }
    for extra in [250.0, 275.0, 300.0, 325.0, 350.0, 2_500.0, 2_750.0, 3_000.0, 3_250.0, 3_500.0] {
        v.push(extra);
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

/// Level of `x` at `freq`, relative to an unprocessed tone of the same
/// amplitude measured the same way.
fn deviation_db(freq: f32, out: &[f32]) -> f32 {
    let lvl = level_db_at(&spectrum(out, FFT), freq, SR, FFT);
    let reference: Vec<f32> = (0..FFT).map(|i| tone_sample(i, freq, SR, AMP)).collect();
    lvl - level_db_at(&spectrum(&reference, FFT), freq, SR, FFT)
}

/// Drive `f` with a `freq` tone, discard the warm-up, return the response in dB.
fn response(freq: f32, mut f: impl FnMut(&[f32], &mut [f32])) -> f32 {
    let n = WARM + FFT;
    let input: Vec<f32> = (0..n).map(|i| tone_sample(i, freq, SR, AMP)).collect();
    let mut out = vec![0.0f32; n];
    let mut i = 0;
    while i < n {
        let b = BLOCK.min(n - i);
        f(&input[i..i + b], &mut out[i..i + b]);
        i += b;
    }
    deviation_db(freq, &out[WARM..])
}

/// A cascade of biquads as one band.
fn band(coeffs: Vec<audio_dsp::BiquadCoefficients>) -> impl FnMut(f32) -> f32 {
    let mut fs: Vec<audio_dsp::BiquadFilter> =
        coeffs.iter().map(|c| audio_dsp::BiquadFilter::new(*c)).collect();
    move |x| {
        let mut s = x;
        for f in fs.iter_mut() { s = f.process_sample(s); }
        s
    }
}

fn pump(c: &mut nullherz_conductor::Conductor, l: &mut [f32], r: &mut [f32]) {
    let n = l.len();
    let inputs: Vec<&[f32]> = vec![];
    let mut outs = vec![l, r];
    let mut lock = c.engine_coordinator.backend_manager.engine_handle.lock();
    let arc = lock.as_mut().expect("engine");
    let p = Arc::as_ptr(arc) as *mut dyn nullherz_traits::RenderingEngine;
    unsafe { (*p).process_block(&inputs, &mut outs, n); }
}

/// Full console response at one frequency. Expensive (a fresh console each
/// time), so the console sweep is sparse and the dense one is on the isolator.
fn console_response(freq: f32) -> f32 {
    let mut c = nullherz_conductor::Conductor::with_library_path(":memory:");
    c.setup_engine();
    c.bootstrap_4channel_mixer();
    let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    for _ in 0..SETTLE_BLOCKS { pump(&mut c, &mut l, &mut r); }

    let frames = SR as usize * 6;
    let mut samples = vec![0.0f32; frames * 2];
    for ch in 0..2 {
        for i in 0..frames {
            samples[ch * frames + i] = tone_sample(i, freq, SR, AMP);
        }
    }
    let mut meta = nullherz_traits::SampleMetadata::new_empty();
    meta.bpm = 120.0;
    meta.total_samples = frames as u64;
    meta.channels = 2;
    meta.sample_rate = SR as u32;
    let id = 8000 + freq as u64;
    c.transfusion_manager.sample_registry
        .register_with_metadata(id, Arc::new(samples), Arc::new(meta));
    c.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: id }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
    ]);
    for _ in 0..SETTLE_BLOCKS { l.fill(0.0); r.fill(0.0); pump(&mut c, &mut l, &mut r); }

    let mut out = Vec::with_capacity(FFT);
    while out.len() < FFT {
        l.fill(0.0); r.fill(0.0);
        pump(&mut c, &mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out.truncate(FFT);
    // The console has a known flat -3.08 dB (crossfader at centre + isolator);
    // this reports RIPPLE, so normalise that away and show deviation from the
    // console's own mean.
    deviation_db(freq, &out)
}

fn bar(dev: f32) -> String {
    // One character per 0.05 dB, centred. Makes a 0.5 dB dip obvious in a column
    // of numbers that all start "-0.".
    let n = ((dev.abs() / 0.05).round() as usize).min(40);
    let c = if dev < 0.0 { '<' } else { '>' };
    std::iter::repeat_n(c, n).collect()
}

fn main() {
    let lp = |hz| audio_dsp::BiquadCoefficients::linkwitz_riley_lp(hz, SR);
    let hp = |hz| audio_dsp::BiquadCoefficients::linkwitz_riley_hp(hz, SR);

    println!("=== DjIsolator at unity: SHIPPED vs CORRECT 3-way nesting ===");
    println!("  shipped: LOW=LP300^2  MID=HP300^2 LP3000^2  HIGH=HP3000^2");
    println!("  correct: LOW=LP300^2  MID=HP300^2 LP3000^2  HIGH=HP300^2 HP3000^2");
    println!();
    println!("  {:>9}  {:>9}  {:>9}   {}", "Hz", "shipped", "correct", "shipped deviation");

    let mut worst = (0.0f32, 0.0f32);
    let mut worst_correct = (0.0f32, 0.0f32);
    for f in sweep_points() {
        let shipped = {
            let mut iso = audio_dsp::DjIsolatorStereo::with_sample_rate(SR);
            let mut scratch = vec![0.0f32; BLOCK];
            response(f, |i, o| iso.process_stereo(i, i, o, &mut scratch[..i.len()]))
        };
        let correct = {
            let mut low = band(vec![lp(300.0), lp(300.0)]);
            let mut mid = band(vec![hp(300.0), hp(300.0), lp(3000.0), lp(3000.0)]);
            let mut high = band(vec![hp(300.0), hp(300.0), hp(3000.0), hp(3000.0)]);
            response(f, |i, o| {
                for (k, &x) in i.iter().enumerate() { o[k] = low(x) + mid(x) + high(x); }
            })
        };
        if shipped.abs() > worst.1.abs() { worst = (f, shipped); }
        if correct.abs() > worst_correct.1.abs() { worst_correct = (f, correct); }
        println!("  {f:>9.0}  {shipped:>+8.3}  {correct:>+8.3}   {}", bar(shipped));
    }
    println!();
    println!("  WORST shipped: {:+.3} dB at {:.0} Hz", worst.1, worst.0);
    println!("  WORST correct: {:+.3} dB at {:.0} Hz", worst_correct.1, worst_correct.0);

    // ---- the full console, sparsely -----------------------------------------
    println!("\n=== full console at unity (deviation from flat) ===");
    println!("  Includes the crossfader's -3.01 dB and the isolator above, so the");
    println!("  absolute figure is offset; what matters is RIPPLE across frequency.");
    println!("  {:>9}  {:>10}", "Hz", "response");
    let mut vals = Vec::new();
    for f in [40.0f32, 100.0, 300.0, 1_000.0, 3_000.0, 6_000.0, 10_000.0, 16_000.0] {
        let d = console_response(f);
        vals.push(d);
        println!("  {f:>9.0}  {d:>+9.3} dB");
    }
    let mean = vals.iter().sum::<f32>() / vals.len() as f32;
    let ripple = vals.iter().fold(0.0f32, |a, v| a.max((v - mean).abs()));
    println!("\n  mean {:+.3} dB, worst ripple about the mean {:+.3} dB", mean, ripple);
}
