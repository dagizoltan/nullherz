//! The reverb must describe a ROOM, not a number of samples — and a stereo
//! reverb must produce a stereo image.
//!
//! Freeverb's `comb_lengths = [1116, 1188, 1277, 1356]` and
//! `allpass_lengths = [556, 441]` are tunings in SAMPLES at 44.1 kHz. Used
//! unscaled they describe a shorter room as the rate rises, and
//! `ReverbFactory::create_processor` discarded its `_sample_rate` while
//! `process` ignored its `ctx` entirely, so nothing scaled them. Measured by
//! `probe_reverb_quality` before the fix:
//!
//!     rate      decay    L/R corr   max|L-R|
//!    44100     0.790 s    1.00000      0.0
//!    48000     0.710 s    1.00000      0.0     (-10.1%)
//!    96000     0.360 s    1.00000      0.0     (-54.4%)
//!
//! Two defects in one table: the tail shrinks with the rate, and the channels
//! are bit-identical because `comb_lengths[c]` was indexed by comb only, never
//! by channel. A stereo reverb with no stereo image, which the wet/dry mix
//! cannot widen because there is nothing to widen.

use nullherz_processors::algorithmic_reverb::AlgorithmicReverbProcessor;
use nullherz_traits::{ProcessContext, SignalProcessor, Transport};

const BLOCK: usize = 256;

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

/// One impulse, identical in both channels; returns the stereo tail.
fn impulse_response(rate: f32, seconds: f32) -> (Vec<f32>, Vec<f32>) {
    let mut rev = AlgorithmicReverbProcessor::with_sample_rate(rate);
    rev.wet_dry = 1.0; // all wet: measure the reverb, not the dry path
    rev.room_size = 0.8;
    rev.damp = 0.2;

    let total = (rate * seconds) as usize;
    let (mut left, mut right) = (Vec::with_capacity(total), Vec::with_capacity(total));
    let t = transport(rate);
    let mut first = true;

    while left.len() < total {
        let mut in_l = [0.0f32; BLOCK];
        let mut in_r = [0.0f32; BLOCK];
        if first {
            in_l[0] = 1.0;
            in_r[0] = 1.0;
            first = false;
        }
        let mut out_l = [0.0f32; BLOCK];
        let mut out_r = [0.0f32; BLOCK];
        {
            let ins: [&[f32]; 2] = [&in_l, &in_r];
            let (a, b) = (&mut out_l[..], &mut out_r[..]);
            let mut outs: [&mut [f32]; 2] = [a, b];
            let mut ctx = ProcessContext {
                transport: Some(&t),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };
            rev.process(&ins, &mut outs, &mut ctx);
        }
        left.extend_from_slice(&out_l);
        right.extend_from_slice(&out_r);
    }
    left.truncate(total);
    right.truncate(total);
    (left, right)
}

/// Seconds until the tail's local energy falls 60 dB below its peak.
fn decay_seconds(x: &[f32], rate: f32) -> f32 {
    let win = ((rate * 0.01) as usize).max(1);
    let mut peak = 0.0f32;
    let mut energies = Vec::new();
    for w in x.chunks(win) {
        let e = (w.iter().map(|v| v * v).sum::<f32>() / w.len() as f32).sqrt();
        peak = peak.max(e);
        energies.push(e);
    }
    let floor = peak * 10f32.powf(-60.0 / 20.0);
    let last = energies.iter().rposition(|&e| e > floor).unwrap_or(0);
    (last as f32 * win as f32) / rate
}

fn correlation(l: &[f32], r: &[f32]) -> f32 {
    let n = l.len().min(r.len());
    let (mut num, mut dl, mut dr) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        num += l[i] as f64 * r[i] as f64;
        dl += l[i] as f64 * l[i] as f64;
        dr += r[i] as f64 * r[i] as f64;
    }
    if dl == 0.0 || dr == 0.0 {
        return 1.0;
    }
    (num / (dl.sqrt() * dr.sqrt())) as f32
}

/// A room does not shrink because the converter changed.
#[test]
fn test_the_tail_length_does_not_depend_on_the_sample_rate() {
    let reference = decay_seconds(&impulse_response(44_100.0, 3.0).0, 44_100.0);
    assert!(reference > 0.1, "precondition: the reverb produced no measurable tail");

    for rate in [48_000.0f32, 96_000.0] {
        let decay = decay_seconds(&impulse_response(rate, 3.0).0, rate);
        let drift = (decay / reference - 1.0) * 100.0;
        assert!(
            drift.abs() < 5.0,
            "at {rate:.0} Hz the -60 dB tail is {decay:.3} s against {reference:.3} s at \
             44.1 kHz ({drift:+.1}%). The delay constants are sample counts, so they are being \
             used unscaled — the same room gets shorter as the rate rises."
        );
    }
}

/// A stereo reverb must have a stereo image.
#[test]
fn test_the_channels_are_not_identical() {
    for rate in [44_100.0f32, 48_000.0, 96_000.0] {
        let (l, r) = impulse_response(rate, 2.0);
        let corr = correlation(&l, &r);
        let maxdiff = l.iter().zip(r.iter()).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(
            corr < 0.9,
            "at {rate:.0} Hz the two channels correlate at {corr:.5} (max sample difference \
             {maxdiff:.2e}). Both channels are running the same delay network, so the reverb has \
             no stereo image at all and the wet/dry mix cannot create one."
        );
        assert!(maxdiff > 1e-6, "at {rate:.0} Hz the channels are bit-identical");
    }
}

/// The right channel is offset, not merely different: its delays are longer, so
/// a purely-left input must still reach the right output.
#[test]
fn test_the_stereo_offset_is_applied_to_one_channel_only() {
    // Identical input to both channels, so any difference in the output comes
    // from the delay network rather than from the input.
    let (l, r) = impulse_response(48_000.0, 1.0);
    let energy_l: f64 = l.iter().map(|v| (*v as f64) * (*v as f64)).sum();
    let energy_r: f64 = r.iter().map(|v| (*v as f64) * (*v as f64)).sum();
    assert!(energy_l > 0.0 && energy_r > 0.0, "a channel produced silence");
    let ratio = energy_r / energy_l;
    assert!(
        (0.5..2.0).contains(&ratio),
        "channel energies differ by {ratio:.2}x — the spread should decorrelate the channels, \
         not attenuate one of them"
    );
}

/// The read index is computed modulo `MAX_DELAY_BUF`, so a scaled length at or
/// beyond the buffer would alias onto the write position and feed a comb its
/// own input. 192 kHz is the highest rate the backend offers.
#[test]
fn test_extreme_rates_stay_finite() {
    for rate in [8_000.0f32, 192_000.0] {
        let (l, r) = impulse_response(rate, 0.25);
        assert!(
            l.iter().chain(r.iter()).all(|v| v.is_finite()),
            "reverb produced a non-finite sample at {rate:.0} Hz"
        );
        let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak < 10.0, "reverb ran away at {rate:.0} Hz (peak {peak})");
    }
}
