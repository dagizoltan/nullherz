//! The analysis kernel must be told what rate the buffer it is analysing was
//! recorded at.
//!
//! Almost every value the kernel derives is scaled by the sample rate: BPM is
//! `sample_rate * 60 / interval`, band-split filter coefficients are relative to
//! it, FFT bins map to frequency through it, and micro-timing deviations are
//! reported in milliseconds. The kernel lives in a `thread_local!` constructed
//! once at 44.1 kHz, so before `set_sample_rate` existed a 48 kHz file was
//! analysed as if it were 44.1 kHz — reporting a BPM 8.8% low.

use nullherz_conductor::analysis_kernel::AnalysisKernel;

/// Build a click track at `bpm` in `rate`-Hz frames: a short impulse burst on
/// every beat, which is what the transient detector keys on.
fn click_track(rate: u32, bpm: f32, beats: usize) -> Vec<f32> {
    let frames_per_beat = (rate as f64 * 60.0 / bpm as f64) as usize;
    let mut buf = vec![0.0f32; frames_per_beat * beats + frames_per_beat];
    for b in 0..beats {
        let at = b * frames_per_beat;
        // A few hundred frames of full-scale noise-ish content per beat gives the
        // FFT-based detector a clear spectral-flux jump.
        for i in 0..512 {
            if at + i < buf.len() {
                let x = ((i * 2654435761usize) % 2048) as f32 / 1024.0 - 1.0;
                buf[at + i] = x * 0.9;
            }
        }
    }
    buf
}

#[test]
fn test_detected_bpm_is_correct_at_each_sample_rate() {
    // The same musical tempo, rendered at three rates. All three must analyse to
    // the same BPM; an unset rate would scale the result by rate/44100.
    let bpm = 120.0f32;
    for rate in [44_100u32, 48_000, 96_000] {
        let buf = click_track(rate, bpm, 32);

        let mut kernel = AnalysisKernel::new(44_100.0); // deliberately wrong at first
        kernel.set_sample_rate(rate as f32);
        let (metadata, _dna) = kernel.analyze(&buf);

        // detect_bpm_from_transients folds into [70, 175), so a correct 120
        // stays 120. Allow a wide tolerance: this asserts the RATE handling, not
        // the accuracy of the detector itself.
        assert!(
            (metadata.bpm - bpm).abs() < bpm * 0.06,
            "at {rate} Hz the kernel reported {:.2} BPM for {bpm} BPM material \
             (tolerance 6%) — rate is not being applied",
            metadata.bpm
        );

        // And the rate must be stamped onto the record, or the frame-denominated
        // fields it just produced (transients, beat grid) are uninterpretable.
        assert_eq!(
            metadata.sample_rate, rate,
            "analysis did not record the rate it analysed at"
        );
    }
}

#[test]
fn test_analysing_48k_as_44k_is_measurably_wrong() {
    // Pins the failure mode the fix addresses: the SAME buffer analysed at the
    // wrong rate reports a proportionally wrong tempo. If this ever stops being
    // true, the kernel has stopped depending on the rate and the plumbing above
    // is no longer load-bearing.
    let buf = click_track(48_000, 120.0, 32);

    let mut right = AnalysisKernel::new(48_000.0);
    let (correct, _) = right.analyze(&buf);

    let mut wrong = AnalysisKernel::new(44_100.0);
    let (mistaken, _) = wrong.analyze(&buf);

    assert!(
        (correct.bpm - mistaken.bpm).abs() > 1.0,
        "analysing at 44.1 kHz vs 48 kHz gave the same BPM ({:.2} vs {:.2}); \
         the rate is not affecting detection",
        correct.bpm, mistaken.bpm
    );
}
