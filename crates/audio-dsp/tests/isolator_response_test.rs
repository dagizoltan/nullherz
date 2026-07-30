//! **The isolator must be magnitude-flat at unity.**
//!
//! `DjIsolator` sits on every deck and runs whether or not anyone touched the
//! EQ, so at unity gains it has to hand the signal back unchanged. It is a
//! 3-band Linkwitz-Riley split and re-sum, and the re-sum is where that can go
//! wrong: an LR pair sums to allpass, but this is not a nested pair — the high
//! band never passes through the 300 Hz section (`LOW = LP300^2`,
//! `MID = HP300^2 LP3000^2`, `HIGH = HP3000^2`), so nothing structurally
//! guarantees reconstruction.
//!
//! Measured, that costs **at most 0.115 dB anywhere in the band**, and the
//! textbook nesting (`HIGH = HP300^2 HP3000^2`) is not uniformly better — it
//! carries the same ~0.11 dB dip at 393 Hz and only wins above 700 Hz. So the
//! structure is left as it is, and this test exists to keep the *consequence*
//! bounded rather than to enforce a particular topology.
//!
//! Swept densely around the crossovers on purpose: a re-sum error lives exactly
//! where the bands hand over, and a coarse log sweep steps straight over it.

use audio_dsp::measurement::{level_db_at, spectrum, tone_sample};

const SR: f32 = 48_000.0;
const FFT: usize = 16_384;
const AMP: f32 = 0.5;
/// An LR4 at 300 Hz rings; the head of the output is its transient, not its
/// response.
const WARM: usize = 16_384;

/// Largest magnitude deviation tolerated at unity, in dB.
///
/// Measured worst case is 0.115 dB. 0.30 leaves room for the crossover
/// frequencies being retuned slightly without tripping, while still failing
/// hard on anything structural: dropping a band, breaking the re-sum, or
/// mis-ordering a cascade all produce dB-level errors, not tenths.
const FLATNESS_DB: f32 = 0.30;

fn response_db(freq: f32) -> f32 {
    let n = WARM + FFT;
    let input: Vec<f32> = (0..n).map(|i| tone_sample(i, freq, SR, AMP)).collect();
    let mut out = vec![0.0f32; n];
    let mut scratch = vec![0.0f32; n];
    let mut iso = audio_dsp::DjIsolatorStereo::with_sample_rate(SR);
    iso.process_stereo(&input, &input, &mut out, &mut scratch);

    let got = level_db_at(&spectrum(&out[WARM..], FFT), freq, SR, FFT);
    let reference: Vec<f32> = (0..FFT).map(|i| tone_sample(i, freq, SR, AMP)).collect();
    got - level_db_at(&spectrum(&reference, FFT), freq, SR, FFT)
}

#[test]
fn test_isolator_is_magnitude_flat_at_unity() {
    // Dense at 300 Hz and 3 kHz, where the bands cross.
    let points: &[f32] = &[
        100.0, 200.0, 250.0, 300.0, 350.0, 400.0, 500.0, 700.0, 997.0,
        1_500.0, 2_000.0, 2_500.0, 3_000.0, 3_500.0, 4_000.0, 6_000.0,
        10_000.0, 16_000.0,
    ];
    let mut worst = (0.0f32, 0.0f32);
    for &f in points {
        let d = response_db(f);
        if d.abs() > worst.1.abs() { worst = (f, d); }
        assert!(
            d.abs() < FLATNESS_DB,
            "DjIsolator at unity is {d:+.3} dB at {f} Hz, past the \
             +/-{FLATNESS_DB} dB limit. This node runs on every deck whether or \
             not the EQ was touched, so at unity it must return the signal. \
             Check the band re-sum: LOW + MID + HIGH has to reconstruct, and a \
             dropped or mis-ordered cascade shows up here as dB, not tenths."
        );
    }
    // Not an assertion — surfaced so a run that passes still reports where the
    // structure is weakest, which is the crossover region and nowhere else.
    println!("worst deviation {:+.3} dB at {:.0} Hz", worst.1, worst.0);
}

/// The measurement must be able to SEE a band error, or the test above passes
/// on an analyser that returns zero.
#[test]
fn test_the_flatness_measurement_detects_a_killed_band() {
    let n = WARM + FFT;
    let freq = 997.0f32;
    let input: Vec<f32> = (0..n).map(|i| tone_sample(i, freq, SR, AMP)).collect();
    let mut out = vec![0.0f32; n];
    let mut scratch = vec![0.0f32; n];

    // Kill the MID band, which is the one carrying 997 Hz. A re-sum this broken
    // must read tens of dB down, not tenths.
    let mut iso = audio_dsp::DjIsolatorStereo::with_sample_rate(SR);
    iso.gains = [1.0, 0.0, 1.0];
    iso.process_stereo(&input, &input, &mut out, &mut scratch);

    let got = level_db_at(&spectrum(&out[WARM..], FFT), freq, SR, FFT);
    let reference: Vec<f32> = (0..FFT).map(|i| tone_sample(i, freq, SR, AMP)).collect();
    let dev = got - level_db_at(&spectrum(&reference, FFT), freq, SR, FFT);
    assert!(
        dev < -20.0,
        "killing the mid band moved 997 Hz by only {dev:+.3} dB. The measurement \
         cannot see a band error, so the flatness test above proves nothing."
    );
}
