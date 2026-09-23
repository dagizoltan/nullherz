//! **What is the resampler's quality ceiling, and what does reaching it cost the
//! audio thread?**
//!
//! `probe_resampler_candidates` answered "which kernel family", and the shipped
//! answer is a 16-tap Kaiser sinc at −92.8 dB THD+N. That is comparable to good
//! converter hardware and 33 dB below audibility for a deck played once — but it
//! sits at the 16-bit dithered noise floor (−93 dB), and a studio resamples the
//! same material repeatedly. This probe asks how much further the same family
//! goes, and whether the answer is affordable ON THE AUDIO THREAD.
//!
//! # Why the shipped table plateaus at −101 dB
//!
//! `resample.rs` measured 8/16/32/64 taps and found 64 WORSE than 32, correctly
//! concluding that something other than the kernel had become the floor. Three
//! limits sit on top of each other at about −100 dB:
//!
//!   * the polyphase table's **linear** interpolation between entries at
//!     `RES = 128`;
//!   * the Kaiser window's sidelobes at **`BETA = 9.0`** (~−100 dB by design);
//!   * f32 accumulation across the taps, which is near −130 dB and not yet
//!     binding.
//!
//! Raising any one alone buys nothing, which is exactly what the 64-tap row
//! shows. This sweeps them together.
//!
//! # The axis the shipped analysis did not try
//!
//! `resample.rs` names raising `RES` as the prerequisite for more taps. That
//! works, and it is the expensive door: the table is 8.1 KB *specifically* so it
//! "fits L1 comfortably next to the audio", and `RES` 128 → 512 makes it 32 KB,
//! evicting the working set it shares L1 with. On an RT thread that trades a
//! smaller mean for a worse TAIL, which is the wrong direction.
//!
//! Raising the interpolation ORDER costs no footprint at all. Linear error
//! scales as h², cubic as h⁴. And on the polyphase fast path it is unusually
//! cheap: at stretch 1.0 every tap shares ONE interpolation weight, so cubic
//! means blending four contiguous coefficient rows instead of two — the input
//! samples load once either way.
//!
//! # What this reports, and why it is not just THD+N
//!
//! A kernel that measures better and evicts the audio from L1 is a worse
//! kernel. Every row carries its table footprint, and the cost column is the
//! per-sample time of the SAME dot product the RT path runs.
//!
//!   cargo run --release -p nullherz-conductor --example probe_resampler_ceiling

use std::time::Instant;

use audio_dsp::measurement::{analyser_floor, thd_n, tone_sample};

const SR: f32 = 48_000.0;
const FFT: usize = 32_768;
const AMP: f32 = 0.5;
const WARM: usize = 8_192;
/// A realistic non-integer tempo ratio — the case that actually resamples.
const RATIO: f64 = 1.0293;

fn db(x: f32) -> f32 { 20.0 * x.max(1e-20).log10() }

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let hx = x / 2.0;
    for k in 1..60 {
        term *= (hx / k as f64) * (hx / k as f64);
        sum += term;
        if term < 1e-20 * sum { break; }
    }
    sum
}

#[derive(Clone, Copy, PartialEq)]
enum Interp { Linear, Cubic }

struct Kernel {
    taps: usize,
    res: usize,
    interp: Interp,
    /// `h(u)` for u in [-taps/2, +taps/2], stored f64 so the TABLE is never the
    /// floor under test. The shipped path stores f32; the f32 column below
    /// reports what that costs.
    h: Vec<f64>,
}

impl Kernel {
    fn new(taps: usize, res: usize, beta: f64, fc: f64, interp: Interp) -> Self {
        let half = taps as f64 / 2.0;
        let n = taps * res + 1;
        let mut h = Vec::with_capacity(n);
        for i in 0..n {
            let u = -half + i as f64 / res as f64;
            let s = {
                let a = std::f64::consts::PI * fc * u;
                if a.abs() < 1e-15 { 1.0 } else { a.sin() / a }
            };
            let r = (u / half).clamp(-1.0, 1.0);
            let w = bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / bessel_i0(beta);
            h.push(fc * s * w);
        }
        Self { taps, res, interp, h }
    }

    /// Bytes the SHIPPED phase-major f32 table would occupy — the number that
    /// decides whether this still shares L1 with the audio.
    fn footprint_bytes(&self) -> usize {
        // polyr is (res + 1) * taps f32 entries (phase res can still read p+1).
        (self.res + 1) * self.taps * 4
    }

    #[inline]
    fn at(&self, u: f64) -> f64 {
        let half = self.taps as f64 / 2.0;
        let pos = (u + half) * self.res as f64;
        if pos <= 0.0 || pos >= (self.h.len() - 1) as f64 { return 0.0; }
        let i = pos as usize;
        let f = pos - i as f64;
        match self.interp {
            Interp::Linear => self.h[i] + (self.h[i + 1] - self.h[i]) * f,
            Interp::Cubic => {
                // Catmull-Rom across table entries. Clamped at the edges, where
                // the kernel is ~0 anyway.
                let im1 = if i == 0 { 0 } else { i - 1 };
                let ip2 = (i + 2).min(self.h.len() - 1);
                let (p0, p1, p2, p3) = (self.h[im1], self.h[i], self.h[i + 1], self.h[ip2]);
                let c1 = p1;
                let c2 = 0.5 * (p2 - p0);
                let c3 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
                let c4 = 0.5 * (p3 - p0) + 1.5 * (p1 - p2);
                ((c4 * f + c3) * f + c2) * f + c1
            }
        }
    }

    /// One output sample at source position `t` with kernel stretch `s`.
    ///
    /// The stretch is the anti-aliasing term and is NOT optional: for rates
    /// above 1.0 the kernel widens to `s * taps` source samples and its cutoff
    /// drops to `fc/s`, which is what stops the decimation aliasing. Omitting it
    /// measures a kernel nobody runs.
    ///
    /// `acc64` selects the accumulator width: f64 finds the KERNEL's own limit,
    /// f32 finds what the shipped SIMD path can actually reach.
    #[inline]
    fn sample(&self, b: &[f32], t: f64, s: f64, acc64: bool) -> f32 {
        let half = self.taps as f64 / 2.0 * s;
        let lo = (t - half).ceil() as isize;
        let hi = (t + half).floor() as isize;
        if acc64 {
            let mut acc = 0.0f64;
            for n in lo..=hi {
                if n < 0 || n as usize >= b.len() { continue; }
                acc += b[n as usize] as f64 * self.at((t - n as f64) / s);
            }
            (acc / s) as f32
        } else {
            let mut acc = 0.0f32;
            for n in lo..=hi {
                if n < 0 || n as usize >= b.len() { continue; }
                acc += b[n as usize] * self.at((t - n as f64) / s) as f32;
            }
            acc / s as f32
        }
    }
}

/// Offset far enough in that a wide kernel is never clamped at the buffer head.
const HEAD: usize = 256;

/// Render `freq` through the kernel at `RATIO`, and return the tail plus the
/// frequency the tone ACTUALLY lands on.
///
/// Reading the source at `RATIO` samples per output sample multiplies the
/// frequency by `RATIO`. Measuring at the input frequency instead analyses a bin
/// the fundamental is not in — 20 bins out at 997 Hz — and returns a number that
/// is not THD+N of anything.
fn render(k: &Kernel, freq: f32, acc64: bool) -> (Vec<f32>, f32) {
    let s = RATIO.max(1.0);
    let need = WARM + FFT;
    let src_len = (need as f64 * RATIO) as usize + 4 * k.taps + 2 * HEAD;
    let src: Vec<f32> = (0..src_len).map(|i| tone_sample(i, freq, SR, AMP)).collect();
    let mut out = Vec::with_capacity(need);
    for i in 0..need {
        out.push(k.sample(&src, HEAD as f64 + i as f64 * RATIO, s, acc64));
    }
    (out[WARM..].to_vec(), freq * RATIO as f32)
}

fn thd_at(k: &Kernel, freq: f32, acc64: bool) -> f32 {
    let (tail, f_out) = render(k, freq, acc64);
    db(thd_n(&tail, f_out, SR, FFT))
}

fn cost_ns(k: &Kernel) -> f64 {
    let src: Vec<f32> = (0..20_000).map(|i| tone_sample(i, 997.0, SR, AMP)).collect();
    let n = 200_000;
    // Warm the table into cache first — we are pricing the kernel, and the
    // cache behaviour is reported separately as footprint.
    let mut sink = 0.0f32;
    let s_stretch = RATIO.max(1.0);
    for i in 0..2_000 { sink += k.sample(&src, HEAD as f64 + i as f64 * RATIO, s_stretch, false); }
    let t0 = Instant::now();
    for i in 0..n { sink += k.sample(&src, HEAD as f64 + (i % 15_000) as f64 * RATIO, s_stretch, false); }
    let el = t0.elapsed().as_nanos() as f64 / n as f64;
    std::hint::black_box(sink);
    el
}

fn main() {
    let floor = analyser_floor(997.0, SR, FFT);
    println!("Analyser floor: {:.1} dB at FFT {}. Nothing below this is resolvable.\n", db(floor), FFT);
    println!("L1d on this class of part is 32 KB, SHARED with the audio working set.");
    println!("The shipped kernel's table is 8.1 KB, chosen for exactly that reason.\n");

    // VALIDATION: the shipped configuration must reproduce the -92.8 dB that
    // resample.rs documents. If it does not, this harness is measuring
    // something else and every row below is noise.
    let shipped = Kernel::new(16, 128, 9.0, 0.90, Interp::Linear);
    let check = thd_at(&shipped, 10_000.0, false);
    println!("VALIDATION: shipped 16t/b9.0/linear at 10 kHz = {check:.1} dB (resample.rs documents -92.8)");
    if (check - -92.8).abs() > 4.0 {
        println!("  ^ DOES NOT MATCH. The harness is wrong, not the kernel. Numbers below are void.\n");
    } else {
        println!("  ^ matches — the harness reproduces the documented figure.\n");
    }

    let cands: Vec<(String, Kernel)> = vec![
        ("SHIPPED 16t b9.0 lin".into(),  Kernel::new(16, 128, 9.0,  0.90, Interp::Linear)),
        ("16t b9.0 CUBIC".into(),        Kernel::new(16, 128, 9.0,  0.90, Interp::Cubic)),
        ("16t b12 cubic".into(),         Kernel::new(16, 128, 12.0, 0.90, Interp::Cubic)),
        ("16t b14 cubic".into(),         Kernel::new(16, 128, 14.0, 0.88, Interp::Cubic)),
        ("24t b12 cubic".into(),         Kernel::new(24, 128, 12.0, 0.90, Interp::Cubic)),
        ("24t b14 cubic".into(),         Kernel::new(24, 128, 14.0, 0.90, Interp::Cubic)),
        ("32t b14 cubic".into(),         Kernel::new(32, 128, 14.0, 0.92, Interp::Cubic)),
        ("32t b16 cubic".into(),         Kernel::new(32, 128, 16.0, 0.92, Interp::Cubic)),
        ("32t b14 LINEAR".into(),        Kernel::new(32, 128, 14.0, 0.92, Interp::Linear)),
        ("32t b14 lin RES512".into(),    Kernel::new(32, 512, 14.0, 0.92, Interp::Linear)),
    ];

    println!("=== THD+N at ratio {RATIO}, f64 accumulate (the KERNEL's own limit) ===");
    println!("  {:<22} {:>10} {:>10} {:>10} {:>10} {:>9}", "kernel", "997 Hz", "5 kHz", "10 kHz", "table", "ns/samp");
    for (name, k) in &cands {
        let t1 = thd_at(k, 997.0, true);
        let t2 = thd_at(k, 5_000.0, true);
        let t3 = thd_at(k, 10_000.0, true);
        let kb = k.footprint_bytes() as f64 / 1024.0;
        let l1 = if kb > 24.0 { " !!" } else if kb > 12.0 { " !" } else { "" };
        println!("  {:<22} {:>9.1} {:>9.1} {:>9.1} {:>7.1} KB{:<2} {:>8.1}",
            name, t1, t2, t3, kb, l1, cost_ns(k));
    }

    println!("\n=== Same kernels, f32 accumulate (what the SIMD path can reach) ===");
    println!("  {:<22} {:>10} {:>10} {:>10}   delta vs f64 at 10 kHz", "kernel", "997 Hz", "5 kHz", "10 kHz");
    for (name, k) in &cands {
        let t1 = thd_at(k, 997.0, false);
        let t2 = thd_at(k, 5_000.0, false);
        let t3 = thd_at(k, 10_000.0, false);
        let t3_64 = thd_at(k, 10_000.0, true);
        println!("  {:<22} {:>9.1} {:>9.1} {:>9.1}   {:>+6.1} dB", name, t1, t2, t3, t3 - t3_64);
    }

    println!("\nReading this: a kernel is only a candidate if it improves THD+N, keeps the");
    println!("table well inside L1, and loses little to f32 accumulation. A row that wins");
    println!("on THD+N with a 32 KB table is not a candidate — it trades mean for tail.");
}
