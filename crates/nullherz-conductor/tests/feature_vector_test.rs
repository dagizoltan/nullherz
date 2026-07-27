//! The band-energy feature vector must actually describe the spectrum.
//!
//! The bands were fixed octaves from 20 Hz to 5.12 kHz. At an FFT size of 1024
//! the bin width is `sample_rate / 1024` — 43 Hz at 44.1 kHz — so:
//!
//!   * the 20-40 Hz band spanned bins 0..0, ZERO bins, making
//!     `feature_vector[0]` structurally 0.0 for every track ever analysed; and
//!   * the top band ended at 5.12 kHz, so bins 118..512 (5 kHz - Nyquist)
//!     contributed to the energy total but to no band at all.
//!
//! One eighth of the vector was a constant and most of the audible spectrum was
//! described by nothing. Because a constant dimension contributes nothing to the
//! cosine similarity in `calculate_similarity`, that also silently reduced the
//! usable feature space.

use nullherz_conductor::analysis_kernel::AnalysisKernel;

const RATE: f32 = 44_100.0;

/// A sine at `hz`, long enough for the analysis passes to have real input.
fn tone(hz: f32, rate: f32, secs: f32) -> Vec<f32> {
    let n = (rate * secs) as usize;
    (0..n)
        .map(|i| (i as f32 * hz * 2.0 * std::f32::consts::PI / rate).sin() * 0.8)
        .collect()
}

#[test]
fn test_no_feature_dimension_is_structurally_dead() {
    // Sweep energy across the spectrum; every band must light up for SOME input.
    // A band that can never be non-zero is a wasted dimension.
    let mut ever_nonzero = [false; 8];

    for hz in [60.0, 120.0, 300.0, 700.0, 1_500.0, 3_000.0, 7_000.0, 15_000.0] {
        let mut kernel = AnalysisKernel::new(RATE);
        let (_meta, dna) = kernel.analyze(&tone(hz, RATE, 1.0));
        for (i, v) in dna.feature_vector.iter().enumerate() {
            if *v > 0.0 {
                ever_nonzero[i] = true;
            }
        }
    }

    for (i, seen) in ever_nonzero.iter().enumerate() {
        assert!(
            seen,
            "feature_vector[{i}] was zero for every test tone across the whole \
             spectrum — the band has no bins and the dimension is dead"
        );
    }
}

#[test]
fn test_low_and_high_content_are_distinguishable() {
    // The vector's job is to describe spectral shape. A bass tone and a treble
    // tone must not produce the same feature vector.
    let mut kernel = AnalysisKernel::new(RATE);
    let (_m1, bass) = kernel.analyze(&tone(80.0, RATE, 1.0));
    let (_m2, treble) = kernel.analyze(&tone(9_000.0, RATE, 1.0));

    let distance: f32 = bass
        .feature_vector
        .iter()
        .zip(treble.feature_vector.iter())
        .map(|(a, b)| (a - b).abs())
        .sum();

    assert!(
        distance > 0.1,
        "an 80 Hz tone and a 9 kHz tone produced near-identical feature vectors \
         ({:?} vs {:?}) — the bands are not resolving the spectrum",
        bass.feature_vector, treble.feature_vector
    );
}

/// Index of the band holding the most energy.
fn peak_band(dna: &nullherz_traits::SoundDNA) -> usize {
    dna.feature_vector
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .expect("feature vector is non-empty")
}

#[test]
fn test_the_top_end_is_resolved_not_just_reached() {
    // Guards the old top-end blind spot: bands stopped at 5.12 kHz, so all
    // content above that fell outside every band.
    //
    // "Non-zero" and even "peaks in a top band" are both too weak to catch it —
    // spectral leakage alone makes the nearest covered band dominate, so a
    // 12 kHz tone still appeared to peak in the top band under the broken
    // layout. The property that actually requires coverage is RESOLUTION: two
    // tones well over an octave apart in the high range must land in DIFFERENT
    // bands. With bands ending at 5.12 kHz they both collapse into the last one.
    let mut kernel = AnalysisKernel::new(RATE);
    let (_m1, mid_high) = kernel.analyze(&tone(3_000.0, RATE, 1.0));
    let (_m2, very_high) = kernel.analyze(&tone(12_000.0, RATE, 1.0));

    let (a, b) = (peak_band(&mid_high), peak_band(&very_high));
    assert!(
        b > a,
        "3 kHz peaked in band {a} and 12 kHz in band {b}; two tones two octaves \
         apart must resolve to different bands, and the higher one must be \
         higher. The layout is not covering the top of the spectrum.\n  \
         3 kHz:  {:?}\n  12 kHz: {:?}",
        mid_high.feature_vector, very_high.feature_vector
    );
}

#[test]
fn test_bands_track_the_sample_rate() {
    // Band edges are derived from bin index, so they scale with the rate. The
    // same musical content at a different rate must still populate the vector.
    for rate in [44_100.0f32, 48_000.0, 96_000.0] {
        let mut kernel = AnalysisKernel::new(rate);
        let (_meta, dna) = kernel.analyze(&tone(1_000.0, rate, 1.0));
        assert!(
            dna.feature_vector.iter().any(|v| *v > 0.0),
            "at {rate} Hz a 1 kHz tone produced an all-zero feature vector"
        );
    }
}
