//! A cutoff in Hz must mean the same thing at every device rate.
//!
//! `NeuralFilterProcessor::process` had `let sample_rate = 48000.0f32;` and
//! ignored its `ctx`, so `w0 = TAU * cutoff / sample_rate` was computed against
//! a constant. The corner therefore moved with the device: a nominal 1 kHz
//! cutoff sat at ~1088 Hz on a 44.1 kHz device and ~500 Hz at 96 kHz.
//!
//! `rate_contract_gate_test` rejects the FORM. This asserts the BEHAVIOUR,
//! because the form can be correct and the arithmetic still wrong.

use nullherz_processors::neural_filter::NeuralFilterProcessor;
use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor, Transport};

const BLOCK: usize = 512;

fn transport(sample_rate: f32) -> Transport {
    Transport {
        bpm: 120.0,
        beat_position: 0.0,
        is_playing: true,
        sample_rate,
        absolute_samples: 0,
        system_time_ns: 0,
        device_time_ns: 0,
    }
}

/// RMS of a steady sine at `freq` after the filter, relative to its input.
fn attenuation_db(rate: f32, cutoff_hz: f32, freq: f32) -> f32 {
    let mut f = NeuralFilterProcessor::new(0);
    f.set_parameter(0, cutoff_hz, 0); // 0 = cutoff
    f.set_parameter(1, 0.7, 0); // resonance, well damped
    f.set_parameter(2, 0.0, 0); // no neural drive: measure the filter alone

    let t = transport(rate);
    let mut in_sum = 0.0f64;
    let mut out_sum = 0.0f64;
    let mut phase = 0.0f32;
    let step = std::f32::consts::TAU * freq / rate;

    // Several blocks: discard the first few so the filter state settles.
    let blocks = 24;
    for b in 0..blocks {
        let mut inp = [0.0f32; BLOCK];
        for s in inp.iter_mut() {
            *s = phase.sin();
            phase += step;
            if phase > std::f32::consts::TAU {
                phase -= std::f32::consts::TAU;
            }
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
            f.process(&ins, &mut outs, &mut ctx);
        }
        if b >= 8 {
            for i in 0..BLOCK {
                in_sum += (inp[i] as f64) * (inp[i] as f64);
                out_sum += (out[i] as f64) * (out[i] as f64);
            }
        }
    }
    let ratio = (out_sum / in_sum.max(1e-30)).sqrt().max(1e-12);
    20.0 * (ratio.log10() as f32)
}

/// One octave above a 1 kHz corner must be attenuated the same at any rate.
///
/// This is the measurement the defect fails: with the rate hardcoded to 48 kHz,
/// the real corner lands elsewhere on every other device, so the attenuation at
/// a fixed 2 kHz differs between rates.
#[test]
fn test_the_cutoff_means_the_same_frequency_at_every_rate() {
    const CUTOFF: f32 = 1_000.0;
    const PROBE: f32 = 2_000.0;

    let reference = attenuation_db(48_000.0, CUTOFF, PROBE);
    assert!(
        reference < -1.0,
        "precondition: a tone an octave above the corner should be attenuated, measured {reference:.2} dB"
    );

    for rate in [44_100.0f32, 96_000.0] {
        let att = attenuation_db(rate, CUTOFF, PROBE);
        let drift = (att - reference).abs();
        assert!(
            drift < 2.0,
            "at {rate:.0} Hz a {PROBE:.0} Hz tone is attenuated {att:.2} dB against {reference:.2} dB \
             at 48 kHz ({drift:.2} dB apart). The cutoff is being computed against a fixed rate, so \
             the corner moves with the device."
        );
    }
}

/// And the corner must still do something: a tone below it passes, a tone well
/// above it does not. Guards against the filter being flat (which would make
/// the test above pass vacuously).
#[test]
fn test_the_filter_actually_filters() {
    for rate in [44_100.0f32, 48_000.0, 96_000.0] {
        let below = attenuation_db(rate, 1_000.0, 200.0);
        let above = attenuation_db(rate, 1_000.0, 8_000.0);
        assert!(
            below > above + 6.0,
            "at {rate:.0} Hz a 200 Hz tone ({below:.2} dB) is not passed appreciably more than an \
             8 kHz tone ({above:.2} dB) against a 1 kHz corner — the filter is not shaping anything, \
             so the rate test above would pass vacuously"
        );
    }
}
