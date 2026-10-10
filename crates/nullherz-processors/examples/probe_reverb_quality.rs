//! Two measurable defects in `AlgorithmicReverbProcessor`.
//!
//! 1. ITS DELAYS ARE HARDCODED AT 44.1 kHz. `comb_lengths = [1116, 1188, 1277,
//!    1356]` and `allpass_lengths = [556, 441]` are Freeverb's constants, which
//!    are tunings in SAMPLES. `ReverbFactory::create_processor` discards its
//!    `_sample_rate` argument, and `process` ignores `ctx` entirely, so nothing
//!    ever scales them. At 48 kHz every delay is 8.8% short and the tail is
//!    correspondingly shorter; at 96 kHz it is half.
//!
//! 2. BOTH CHANNELS ARE IDENTICAL. `comb_lengths[c]` is indexed by comb only,
//!    never by channel, so left and right run the same delay network. A stereo
//!    reverb with zero stereo width: a mono source in produces a mono image
//!    out, and the dry/wet mix cannot widen it.
//!
//! This probe measures both so the fix has a before and an after, rather than
//! being asserted. Run:
//!
//!   cargo run --release -p nullherz-processors --example probe_reverb_quality

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

/// Feed one impulse (identical in both channels) and capture the stereo tail.
fn impulse_response(rate: f32, seconds: f32) -> (Vec<f32>, Vec<f32>) {
    let mut rev = AlgorithmicReverbProcessor::new();
    // All wet, so the measurement is the reverb and not the dry path.
    rev.wet_dry = 1.0;
    rev.room_size = 0.8;
    rev.damp = 0.2;

    let total = (rate * seconds) as usize;
    let (mut left, mut right) = (Vec::with_capacity(total), Vec::with_capacity(total));
    let t = transport(rate);
    let mut fed = 0usize;

    while left.len() < total {
        let mut in_l = [0.0f32; BLOCK];
        let mut in_r = [0.0f32; BLOCK];
        if fed == 0 {
            in_l[0] = 1.0;
            in_r[0] = 1.0;
        }
        fed += 1;

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

/// Seconds until the tail's local energy falls below -60 dB of its peak.
fn decay_seconds(x: &[f32], rate: f32) -> f32 {
    let win = (rate * 0.01) as usize; // 10 ms windows
    let mut peak = 0.0f32;
    let mut energies = Vec::new();
    for w in x.chunks(win.max(1)) {
        let e = (w.iter().map(|v| v * v).sum::<f32>() / w.len() as f32).sqrt();
        peak = peak.max(e);
        energies.push(e);
    }
    let floor = peak * 10f32.powf(-60.0 / 20.0);
    let last_above = energies.iter().rposition(|&e| e > floor).unwrap_or(0);
    (last_above as f32 * win as f32) / rate
}

/// 1.0 = the two channels are the same signal (no stereo width at all).
fn correlation(l: &[f32], r: &[f32]) -> f32 {
    let n = l.len().min(r.len());
    let (mut num, mut dl, mut dr) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        num += (l[i] as f64) * (r[i] as f64);
        dl += (l[i] as f64) * (l[i] as f64);
        dr += (r[i] as f64) * (r[i] as f64);
    }
    if dl == 0.0 || dr == 0.0 {
        return 1.0;
    }
    (num / (dl.sqrt() * dr.sqrt())) as f32
}

fn main() {
    println!("reverb quality probe\n");
    println!(
        "{:>10}  {:>14}  {:>13}  {:>12}  {}",
        "rate", "decay (s)", "L/R corr", "max |L-R|", "reading"
    );
    println!("{}", "-".repeat(78));

    let mut decays = Vec::new();
    for rate in [44_100.0f32, 48_000.0, 96_000.0] {
        let (l, r) = impulse_response(rate, 3.0);
        let decay = decay_seconds(&l, rate);
        let corr = correlation(&l, &r);
        let maxdiff = l
            .iter()
            .zip(r.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        decays.push((rate, decay));
        let note = if corr > 0.999 { "channels IDENTICAL" } else { "channels differ" };
        println!("{rate:>10.0}  {decay:>14.3}  {corr:>13.5}  {maxdiff:>12.2e}  {note}");
    }

    let (r0, d0) = decays[0];
    println!("\nRate dependence of the tail (should be flat — a room does not shrink\nbecause the converter changed):");
    for (rate, d) in &decays {
        let pct = (d / d0 - 1.0) * 100.0;
        println!("  {rate:>8.0} Hz  decay {d:>6.3} s   {pct:>+7.1}% vs {r0:.0} Hz");
    }
}
