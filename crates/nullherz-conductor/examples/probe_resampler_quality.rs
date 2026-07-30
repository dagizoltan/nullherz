//! **What does the resampler cost?**
//!
//! Every fidelity number measured so far was taken at playback rate 1.0, where
//! the interpolator is EXACTLY transparent: the playhead lands on whole
//! samples, the fractional part is zero, and `interpolate_sample` returns an
//! input sample verbatim. That is why the console measured -107 dB. A DJ deck
//! spends almost none of its life at rate 1.0 — tempo sync, the pitch fader and
//! key transposition all resample continuously.
//!
//! `SamplerVoice` uses 4-point Lagrange (Catmull-Rom) interpolation. This probe
//! drives it standalone on the shipped analyser (`audio_dsp::measurement`).
//!
//! # The trap in this measurement
//!
//! **Simple rates are transparent and will tell you the resampler is perfect.**
//! At rate 2.0 the fraction is always 0 — every output sample IS an input
//! sample, so a 2.0 test measures nothing at all. At 1.5 the fraction only ever
//! takes two values, at 1.25 four. The interpolator is only fully exercised by a
//! rate whose fractional part has a long period, which is what real tempo
//! ratios look like. Measuring at 2.0 and concluding "clean" is the same
//! mistake as putting the test tone on a bin centre; the rate column below is
//! sorted to make it visible rather than to hide it.
//!
//! Two more, both consequences of the pitch shifting:
//!   * the OUTPUT frequency is `f * rate`, so the fundamental moves. Track it or
//!     the analyser counts the signal as distortion.
//!   * above rate 1.0 the source is being decimated, so source content above
//!     `Nyquist / rate` folds back. That is a separate defect from
//!     interpolation error and needs its own test.
//!
//!   cargo run --release -p nullherz-conductor --example probe_resampler_quality

use std::sync::Arc;

use audio_dsp::measurement::{bin_of, level_db_at, spectrum, thd_n, tone_sample};
use audio_dsp::{InterpolationType, SamplerVoice};

const SR: f32 = 48_000.0;
const FFT: usize = 16_384;
const AMP: f32 = 0.5;
/// Skipped before capture: the voice's first samples sit at the buffer start
/// where the 4-point kernel is clamped, which is a boundary artefact rather
/// than steady-state resampling.
const WARM: usize = 8_192;

fn db(x: f32) -> f32 { 20.0 * x.max(1e-20).log10() }

/// Render a `freq` Hz tone through one voice at `rate`, mono, and return the
/// steady-state tail. The source is generated long enough that the voice never
/// runs dry (it deactivates `4` frames from the end).
fn resample(freq: f32, rate: f32, interp: InterpolationType) -> Vec<f32> {
    let need = WARM + FFT;
    let frames = (need as f32 * rate).ceil() as usize + 64;
    let src: Arc<Vec<f32>> =
        Arc::new((0..frames).map(|i| tone_sample(i, freq, SR, AMP)).collect());

    let mut voice = SamplerVoice::new();
    voice.interpolation = interp;
    voice.set_layout(frames, 1);
    voice.trigger_at_ref(&src, rate, 1.0, 0.0, 0.0);

    // `process_block_planar` ACCUMULATES into the output, so it must start
    // zeroed — a dirty buffer would read as the resampler adding energy.
    let mut out = vec![0.0f32; need];
    {
        let mut slice = &mut out[..];
        let mut outs: [&mut [f32]; 1] = [&mut slice];
        voice.process_block_planar(&mut outs, need);
    }
    assert!(voice.is_active, "voice ran dry at rate {rate} — source too short");
    out[WARM..].to_vec()
}

/// How many distinct fractional phases a rate visits before repeating. A rate
/// the interpolator barely exercises has a tiny number here, and that is
/// precisely when it measures clean for the wrong reason.
fn phase_count(rate: f64) -> String {
    let mut seen = std::collections::BTreeSet::new();
    let mut p = 0.0f64;
    for _ in 0..4096 {
        p = (p + rate).fract();
        // Quantised: we want "how many phases does it effectively visit", not
        // an f64 equality count.
        seen.insert((p * 4096.0).round() as i64);
        if seen.len() > 512 { return ">512".into(); }
    }
    format!("{}", seen.len())
}

fn sweep(label: &str, freq: f32, interp: InterpolationType) {
    println!("\n=== {label} — {freq} Hz source ===");
    println!("  {:<26} {:>8}  {:>12}  {:>9}  {:>7}", "rate", "phases", "THD+N", "dB", "level");
    // Sorted from "transparent by construction" to "what a deck actually does".
    let rates: &[(f32, &str)] = &[
        (1.0, "1.0  (identity)"),
        (2.0, "2.0  (octave up, exact)"),
        (0.5, "0.5  (octave down)"),
        (1.5, "1.5  (perfect fifth)"),
        (1.25, "1.25 (major third)"),
        (1.059_463_1, "1.0595 (+1 semitone)"),
        (0.793_700_5, "0.7937 (-4 semitones)"),
        (1.029_302_2, "1.0293 (+2.5% tempo)"),
        (1.008_137_5, "1.0081 (+0.8% nudge)"),
    ];
    for (rate, name) in rates {
        let out = resample(freq, *rate, interp);
        let f_out = freq * rate;
        let t = thd_n(&out, f_out, SR, FFT);
        let mags = spectrum(&out, FFT);
        let lvl = level_db_at(&mags, f_out, SR, FFT);
        // Reference: the same tone at the same OUTPUT frequency, unresampled.
        let refr: Vec<f32> = (0..FFT).map(|i| tone_sample(i, f_out, SR, AMP)).collect();
        let ref_lvl = level_db_at(&spectrum(&refr, FFT), f_out, SR, FFT);
        println!(
            "  {name:<26} {:>8}  {:>11.6}%  {:>8.1}  {:+6.2} dB",
            phase_count(*rate as f64), t * 100.0, db(t), lvl - ref_lvl
        );
    }
}

fn main() {
    println!("Analyser floor: {:.7}% ({:.1} dB) — every number below reads against it.",
        audio_dsp::measurement::analyser_floor(997.0, SR, FFT) * 100.0,
        db(audio_dsp::measurement::analyser_floor(997.0, SR, FFT)));

    sweep("Lagrange (SHIPPED DEFAULT)", 997.0, InterpolationType::Lagrange);
    sweep("Linear (the other option)", 997.0, InterpolationType::Linear);

    // Interpolation error grows with how much of the band the signal occupies:
    // a cubic kernel tracks a slow sine almost exactly and a fast one poorly.
    // A bass-heavy 997 Hz test flatters it.
    sweep("Lagrange at 5 kHz", 5_000.0, InterpolationType::Lagrange);
    sweep("Lagrange at 10 kHz", 10_000.0, InterpolationType::Lagrange);

    // ---- decimation aliasing, a separate defect ----------------------------
    // Above rate 1.0 the source is read faster than it was written, so content
    // above Nyquist/rate has nowhere to go but back down. A correct pitch-up
    // path low-passes the SOURCE first; interpolation quality cannot fix this
    // because the aliasing happens in the resampling itself.
    // A 16 kHz source needs rate > 1.5 to be pushed past Nyquist, so the first
    // row is the control: same tone, same interpolator, no fold available.
    println!("\n=== decimation aliasing above rate 1.0 ===");
    println!("  16 kHz source. It should arrive at 16000*rate; past {:.0} Hz there is", SR / 2.0);
    println!("  nowhere for it to go, and a resampler with no source-side low-pass");
    println!("  folds it back down instead of losing it.");
    println!("  {:<10} {:>11}  {:>12}  {:>12}  {:>10}", "rate", "should be", "loudest at", "vs source", "verdict");
    let f_src = 16_000.0f32;
    // The unresampled tone, measured identically, is the 0 dB reference.
    let ref_lvl = {
        let r: Vec<f32> = (0..FFT).map(|i| tone_sample(i, f_src, SR, AMP)).collect();
        level_db_at(&spectrum(&r, FFT), f_src, SR, FFT)
    };
    for rate in [1.2f32, 1.6, 1.8, 2.0, 2.2] {
        let out = resample(f_src, rate, InterpolationType::Lagrange);
        let mags = spectrum(&out, FFT);
        let ideal = f_src * rate;
        // Loudest bin in the output — where the energy ACTUALLY went, rather
        // than where a fold was predicted. Skip DC and the few bins above it.
        let (pk_bin, _) = mags.iter().enumerate().skip(4)
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap();
        let pk_hz = pk_bin as f32 * SR / FFT as f32;
        let pk_lvl = level_db_at(&mags, pk_hz, SR, FFT);
        let verdict = if ideal <= SR / 2.0 {
            if (pk_hz - ideal).abs() < 20.0 { "ok" } else { "MISPLACED" }
        } else if (pk_hz - ideal).abs() < 20.0 { "ok" } else { "ALIASED" };
        println!(
            "  r={rate:<8.1} {:>8.0} Hz  {:>9.0} Hz  {:>9.1} dB  {:>10}",
            ideal, pk_hz, pk_lvl - ref_lvl, verdict
        );
        let _ = bin_of(ideal, SR, FFT);
    }

    println!("\nReading it: rate 1.0 and 2.0 are transparent BY CONSTRUCTION (fraction");
    println!("always 0) and prove nothing. The rates with a long phase count are the");
    println!("ones a tempo-synced deck actually runs at, and they are the honest number.");
}
