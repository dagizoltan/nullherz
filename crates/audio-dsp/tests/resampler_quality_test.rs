//! **The resampler must be bandlimited, and unity must stay exact.**
//!
//! These pin the two properties the sinc kernel was adopted for, and one the
//! console's whole fidelity story rests on.
//!
//! Note what does NOT cover this: the golden master render plays every deck at
//! playback rate 1.0 (track BPM equals transport BPM), where the interpolator
//! short-circuits to a verbatim sample copy. Swapping the entire resampler
//! kernel left that fixture BYTE IDENTICAL — it cannot see the resampler at all.
//! (The fixture has since moved for an unrelated crossfader change; the point
//! stands, and it is a coverage gap rather than a reassurance.) Everything here
//! is therefore load-bearing on its own.

use std::sync::Arc;

use audio_dsp::measurement::{level_db_at, spectrum, thd_n, tone_sample};
use audio_dsp::{InterpolationType, SamplerVoice};

const SR: f32 = 48_000.0;
const FFT: usize = 16_384;
const AMP: f32 = 0.5;
const WARM: usize = 8_192;

/// A rate whose fractional part has a LONG period, i.e. one that actually
/// exercises the kernel.
///
/// Not 1.5, 1.25 or 2.0. At rate 2.0 the fraction is always zero and every
/// output sample IS an input sample, so it measures the analyser floor whatever
/// the kernel is; 1.5 and 0.5 visit two fractional phases and read 18 dB
/// optimistic. Testing a resampler at a simple ratio proves nothing, which is
/// the same error as putting a test tone on an FFT bin centre.
const TEMPO_RATE: f32 = 1.029_302_2;

fn db(x: f32) -> f32 { 20.0 * x.max(1e-20).log10() }

/// Steady-state output of one voice playing a `freq` tone at `rate`.
///
/// `interp: None` leaves the voice's DEFAULT interpolation alone, which is what
/// the quality assertions below must use. Forcing `Some(Sinc)` in them would
/// pin the sinc kernel's quality while saying nothing about whether the sampler
/// actually uses it — the tests would then pass unchanged with the default
/// reverted to the cubic, which is not a test.
fn resample(freq: f32, rate: f32, interp: Option<InterpolationType>) -> Vec<f32> {
    let need = WARM + FFT;
    let frames = (need as f32 * rate).ceil() as usize + 256;
    let src: Arc<Vec<f32>> =
        Arc::new((0..frames).map(|i| tone_sample(i, freq, SR, AMP)).collect());
    let mut voice = SamplerVoice::new();
    if let Some(i) = interp { voice.interpolation = i; }
    voice.set_layout(frames, 1);
    voice.trigger_at_ref(&src, rate, 1.0, 0.0, 0.0);

    // `process_block_planar` ACCUMULATES; a dirty buffer would read as the
    // resampler adding energy.
    let mut out = vec![0.0f32; need];
    {
        let mut sl = &mut out[..];
        let mut outs: [&mut [f32]; 1] = [&mut sl];
        voice.process_block_planar(&mut outs, need);
    }
    assert!(voice.is_active, "voice ran dry at rate {rate}; source too short");
    out[WARM..].to_vec()
}

/// THE headline guard. Resampling must not add audible distortion at ANY source
/// frequency.
///
/// The old Catmull-Rom cubic measured -29.0 dB here at 10 kHz — 3.56%, which is
/// 35x past the ~0.1% audible threshold on music. The threshold is set at
/// -70 dB: 23 dB of margin over the sinc kernel's measured -92.8 dB, and it
/// fails the cubic by 41 dB.
#[test]
fn test_resampling_is_bandlimited_at_every_frequency() {
    for freq in [997.0f32, 5_000.0, 10_000.0] {
        let out = resample(freq, TEMPO_RATE, None);
        let t = thd_n(&out, freq * TEMPO_RATE, SR, FFT);
        assert!(
            db(t) < -70.0,
            "resampling a {freq} Hz tone at {TEMPO_RATE} measures {:.4}% THD+N \
             ({:.1} dB), past the -70 dB limit. Interpolation error grows with \
             how much of the band the signal occupies, so check the HIGH \
             frequencies first — a kernel can look perfect at 997 Hz and be \
             35x past the audible threshold at 10 kHz.",
            t * 100.0, db(t)
        );
    }
}

/// The property that actually distinguishes a bandlimited kernel: THD+N must not
/// DEGRADE with source frequency.
///
/// This is the test that no polynomial interpolator can pass, and it is why the
/// choice was a windowed sinc rather than simply more Lagrange points. Measured
/// spread across 997 Hz -> 10 kHz: sinc 2.1 dB, Catmull-Rom 62.8 dB, 6-point
/// Lagrange 90 dB. An absolute threshold alone would let a very good polynomial
/// squeak through at low frequencies; this states the design intent directly.
#[test]
fn test_resampler_quality_is_flat_across_frequency() {
    let low = thd_n(&resample(997.0, TEMPO_RATE, None), 997.0 * TEMPO_RATE, SR, FFT);
    let high = thd_n(&resample(10_000.0, TEMPO_RATE, None), 10_000.0 * TEMPO_RATE, SR, FFT);
    let spread = db(high) - db(low);
    assert!(
        spread < 15.0,
        "resampler THD+N degrades {spread:.1} dB from 997 Hz ({:.1} dB) to \
         10 kHz ({:.1} dB). A bandlimited kernel is FLAT across frequency; a \
         polynomial one collapses at HF. This spread says the kernel went back \
         to being a polynomial.",
        db(low), db(high)
    );
}

/// Playback rate 1.0 must stay the BIT-EXACT identity.
///
/// The console's measured -107 dB transparency at unity depends on this, and it
/// is not automatic: a windowed sinc has `h(0) < 1` and `h(+/-1) != 0`, so
/// convolving at an integer position is a mild low-pass rather than a copy.
/// `SamplerVoice::sinc_sample` short-circuits that case; this is what stops the
/// short-circuit being deleted as a micro-optimisation.
#[test]
fn test_rate_one_is_bit_exact_identity() {
    let frames = 8192usize;
    let src: Arc<Vec<f32>> =
        Arc::new((0..frames).map(|i| tone_sample(i, 997.0, SR, AMP)).collect());
    let mut voice = SamplerVoice::new();
    voice.set_layout(frames, 1);
    voice.trigger_at_ref(&src, 1.0, 1.0, 0.0, 0.0);

    let n = frames - 16;
    let mut out = vec![0.0f32; n];
    {
        let mut sl = &mut out[..];
        let mut outs: [&mut [f32]; 1] = [&mut sl];
        voice.process_block_planar(&mut outs, n);
    }
    for i in 0..n {
        assert_eq!(
            out[i].to_bits(), src[i].to_bits(),
            "sample {i} differs at playback rate 1.0 ({} vs {}). Unity playback \
             must be a verbatim copy — the console's -107 dB transparency at \
             unity is measured through this path, and a kernel that filters \
             here turns 'transparent at unity' into a lie.",
            out[i], src[i]
        );
    }
}

/// Pitching UP decimates the source, so content above `Nyquist / rate` must be
/// low-passed away rather than folded back.
///
/// Before the sinc kernel, a 16 kHz tone at rate 2.0 arrived at 16 kHz at
/// **0.0 dB** — the alias was the loudest thing in the output, because no
/// polynomial kernel can low-pass. Now measured 90.9 dB down. The threshold is
/// 40 dB, which the old cubic fails by the full 40.
#[test]
fn test_pitch_up_does_not_alias_at_full_level() {
    let f_src = 16_000.0f32;
    let rate = 2.0f32;
    let out = resample(f_src, rate, None);
    let mags = spectrum(&out, FFT);

    // 16 kHz * 2 = 32 kHz, which folds about Nyquist back to 48000-32000 =
    // 16 kHz — the fold lands back on the source frequency.
    let fold_hz = SR - f_src * rate;
    let fold = level_db_at(&mags, fold_hz, SR, FFT);
    let reference: Vec<f32> = (0..FFT).map(|i| tone_sample(i, f_src, SR, AMP)).collect();
    let source = level_db_at(&spectrum(&reference, FFT), f_src, SR, FFT);

    assert!(
        fold - source < -40.0,
        "a {f_src} Hz tone played at rate {rate} should land at {} Hz, past \
         Nyquist. Instead {:.0} Hz carries {:.1} dB relative to the source — it \
         folded back rather than being filtered away. Interpolation quality \
         cannot fix this; the kernel has to low-pass with the rate.",
        f_src * rate, fold_hz, fold - source
    );
}

/// A guard on the guard: the analyser must still see distortion when it is
/// there. Every assertion above is a "this is clean" claim, and all of them pass
/// trivially against an analyser that returns zero or a voice that renders
/// silence.
#[test]
fn test_the_measurement_still_detects_a_bad_kernel() {
    let out = resample(10_000.0, TEMPO_RATE, Some(InterpolationType::Linear));
    let peak = out.iter().fold(0.0f32, |a, v| a.max(v.abs()));
    assert!(peak > AMP * 0.5, "voice rendered near-silence (peak {peak:.6})");
    let t = thd_n(&out, 10_000.0 * TEMPO_RATE, SR, FFT);
    assert!(
        db(t) > -40.0,
        "linear interpolation at 10 kHz measured {:.1} dB — it should be around \
         -22 dB. The measurement has gone blind, so every 'clean' result in this \
         file is worthless.",
        db(t)
    );
}
