//! **Scoping: what should the resampler be?**
//!
//! Prices up candidate interpolation kernels on quality AND CPU before anything
//! is committed to `SamplerVoice`. The shipped kernel measures -29 dB THD+N at
//! 10 kHz at a realistic tempo ratio; the question is what replaces it and at
//! what cost, and the interesting answer is the KNEE of the curve, not the best
//! number.
//!
//! Every kernel here is a standalone function over a source buffer so the
//! comparison is apples-to-apples and nothing depends on voice plumbing.
//!
//! # The kernels
//!
//! * **Linear** — 2-point. The other shipped option, as a floor.
//! * **Catmull-Rom** — 4-point. THE CURRENT DEFAULT. Note it is named
//!   `InterpolationType::Lagrange` in the code but the coefficients
//!   (`c2 = 0.5*(p2-p0)`) are Catmull-Rom, which is cubic Hermite with tangents
//!   estimated from neighbours — NOT the 4-point Lagrange polynomial. Both are
//!   measured here because true Lagrange is a drop-in with identical support.
//! * **Lagrange 4 / 6** — the actual interpolating polynomials.
//! * **B-spline cubic** — approximating rather than interpolating; included
//!   because it is the standard "smooth" choice and its passband droop is the
//!   reason not to use it.
//! * **2x oversample + Catmull-Rom** — halfband upsample, then the current
//!   kernel on the denser grid.
//! * **Windowed sinc** — Kaiser-windowed, table-driven, 8/16/32/64 taps.
//!   Handles decimation in the same kernel by stretching (see `SincTable`).
//!
//!   cargo run --release -p nullherz-conductor --example probe_resampler_candidates

use std::time::Instant;

use audio_dsp::measurement::{level_db_at, spectrum, thd_n, tone_sample};

const SR: f32 = 48_000.0;
const FFT: usize = 16_384;
const AMP: f32 = 0.5;
const WARM: usize = 8_192;

fn db(x: f32) -> f32 { 20.0 * x.max(1e-20).log10() }

// ---------------------------------------------------------------------------
// Polynomial kernels. Each takes the source, an integer index and a fraction in
// [0,1) positioned between `idx` and `idx+1`, and returns one output sample.
// ---------------------------------------------------------------------------

fn k_linear(b: &[f32], idx: usize, x: f32) -> f32 {
    b[idx] + (b[idx + 1] - b[idx]) * x
}

/// The shipped kernel, transcribed from `SamplerVoice::interpolate_sample`.
fn k_catmull(b: &[f32], idx: usize, x: f32) -> f32 {
    let (p0, p1, p2, p3) = (b[idx - 1], b[idx], b[idx + 1], b[idx + 2]);
    let c1 = p1;
    let c2 = 0.5 * (p2 - p0);
    let c3 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
    let c4 = 0.5 * (p3 - p0) + 1.5 * (p1 - p2);
    ((c4 * x + c3) * x + c2) * x + c1
}

/// Lagrange polynomial through `n` points centred on the fraction. Offsets run
/// from `-(n/2 - 1)` to `n/2`, so `x` in [0,1) sits between points `n/2-1` and
/// `n/2`. O(n^2) — this is a scoping harness, not the shipped path.
fn k_lagrange(b: &[f32], idx: usize, x: f32, n: usize) -> f32 {
    let half = (n / 2) as isize;
    let mut acc = 0.0f64;
    for i in 0..n {
        let oi = i as isize - (half - 1);
        let mut w = 1.0f64;
        for j in 0..n {
            if i == j { continue; }
            let oj = j as isize - (half - 1);
            w *= (x as f64 - oj as f64) / (oi - oj) as f64;
        }
        acc += b[(idx as isize + oi) as usize] as f64 * w;
    }
    acc as f32
}

/// Cubic B-spline: approximating, so it does not pass through its control
/// points. Smooth, and audibly dull for exactly that reason.
fn k_bspline(b: &[f32], idx: usize, x: f32) -> f32 {
    let (p0, p1, p2, p3) = (b[idx - 1], b[idx], b[idx + 1], b[idx + 2]);
    let (t, t2) = (x, x * x);
    let t3 = t2 * t;
    let w0 = (1.0 - 3.0 * t + 3.0 * t2 - t3) / 6.0;
    let w1 = (4.0 - 6.0 * t2 + 3.0 * t3) / 6.0;
    let w2 = (1.0 + 3.0 * t + 3.0 * t2 - 3.0 * t3) / 6.0;
    let w3 = t3 / 6.0;
    p0 * w0 + p1 * w1 + p2 * w2 + p3 * w3
}

// ---------------------------------------------------------------------------
// Windowed sinc, table driven.
// ---------------------------------------------------------------------------

/// Kaiser-windowed sinc, sampled `RES` times per source-sample of support and
/// linearly interpolated between table entries.
///
/// Decimation is handled by STRETCHING the same table rather than by holding a
/// second one. For a stretch `s`, `h_s(u) = (1/s) * h(u/s)`: evaluating the
/// kernel at `u/s` scales its cutoff to `fc/s` and widens its support to
/// `s * taps` source samples, which is exactly the anti-alias low-pass that
/// `rate > 1` needs. One table, both jobs — the property that argues for this
/// family over any polynomial kernel, since no polynomial can low-pass.
struct SincTable {
    taps: usize,
    /// Table entries per unit of `u`.
    res: usize,
    /// `h(u)` for u in [-taps/2, +taps/2].
    h: Vec<f32>,
    /// Normalised cutoff, in units of fs (1.0 == Nyquist).
    fc: f32,
}

impl SincTable {
    fn new(taps: usize, beta: f64, fc: f32) -> Self {
        const RES: usize = 128;
        let half = taps as f64 / 2.0;
        let n = taps * RES + 1;
        let mut h = Vec::with_capacity(n);
        for i in 0..n {
            let u = -half + i as f64 / RES as f64;
            let s = {
                let a = std::f64::consts::PI * fc as f64 * u;
                if a.abs() < 1e-12 { 1.0 } else { a.sin() / a }
            };
            // Kaiser window over |u| <= half.
            let r = (u / half).clamp(-1.0, 1.0);
            let w = bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / bessel_i0(beta);
            h.push((fc as f64 * s * w) as f32);
        }
        Self { taps, res: RES, h, fc }
    }

    #[inline]
    fn at(&self, u: f32) -> f32 {
        let half = self.taps as f32 / 2.0;
        let pos = (u + half) * self.res as f32;
        if pos <= 0.0 || pos >= (self.h.len() - 1) as f32 { return 0.0; }
        let i = pos as usize;
        let f = pos - i as f32;
        self.h[i] + (self.h[i + 1] - self.h[i]) * f
    }

    /// One output sample at source position `t`, with kernel stretch `s`.
    #[inline]
    fn sample(&self, b: &[f32], t: f64, s: f32) -> f32 {
        let half = self.taps as f32 / 2.0 * s;
        let lo = (t - half as f64).ceil() as isize;
        let hi = (t + half as f64).floor() as isize;
        let mut acc = 0.0f32;
        for n in lo..=hi {
            if n < 0 || n as usize >= b.len() { continue; }
            let u = ((t - n as f64) / s as f64) as f32;
            acc += b[n as usize] * self.at(u);
        }
        acc / s
    }
}

/// Modified Bessel function of the first kind, order 0 — the Kaiser window's
/// shape term. Series converges fast for the beta range in use.
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let hx = x / 2.0;
    for k in 1..40 {
        term *= (hx / k as f64) * (hx / k as f64);
        sum += term;
        if term < 1e-18 * sum { break; }
    }
    sum
}

// ---------------------------------------------------------------------------
// Candidate dispatch
// ---------------------------------------------------------------------------

enum Cand {
    Linear,
    Catmull,
    Lagrange(usize),
    BSpline,
    Oversample2x,
    Sinc(SincTable),
}

impl Cand {
    fn name(&self) -> String {
        match self {
            Cand::Linear => "Linear (2-pt)".into(),
            Cand::Catmull => "Catmull-Rom 4-pt (CURRENT)".into(),
            Cand::Lagrange(n) => format!("Lagrange {n}-pt"),
            Cand::BSpline => "B-spline cubic".into(),
            Cand::Oversample2x => "2x oversample + Catmull".into(),
            Cand::Sinc(t) => format!("Sinc {} tap (fc {:.2})", t.taps, t.fc),
        }
    }

    /// Source samples touched per output sample, at stretch `s`. The cost proxy.
    fn support(&self, s: f32) -> f32 {
        match self {
            Cand::Linear => 2.0,
            Cand::Catmull | Cand::BSpline => 4.0,
            Cand::Lagrange(n) => *n as f32,
            Cand::Oversample2x => 8.0,
            Cand::Sinc(t) => t.taps as f32 * s,
        }
    }

    /// Render `out.len()` samples starting from source position `t0`.
    ///
    /// `t0` must be clear of the buffer head: the polynomial kernels read
    /// `idx - 1` (or further back) without bounds checks, exactly as the shipped
    /// one does, so starting at 0 underflows. That is also a real boundary case
    /// in the voice, but it is a boundary artefact rather than steady-state
    /// resampling and does not belong in these numbers.
    fn render(&self, src: &[f32], rate: f32, out: &mut [f32], t0: f64) {
        // 2x oversampling needs a denser source; build it once per render.
        let up: Vec<f32> = if matches!(self, Cand::Oversample2x) {
            halfband_upsample_2x(src)
        } else {
            Vec::new()
        };
        let s = rate.max(1.0);
        let mut t = t0;
        for o in out.iter_mut() {
            match self {
                Cand::Sinc(tab) => { *o = tab.sample(src, t, s); }
                Cand::Oversample2x => {
                    let t2 = t * 2.0;
                    let idx = t2.floor() as usize;
                    let x = (t2 - idx as f64) as f32;
                    *o = if idx >= 1 && idx + 2 < up.len() { k_catmull(&up, idx, x) } else { 0.0 };
                }
                _ => {
                    let idx = t.floor() as usize;
                    let x = (t - idx as f64) as f32;
                    *o = match self {
                        Cand::Linear => k_linear(src, idx, x),
                        Cand::Catmull => k_catmull(src, idx, x),
                        Cand::Lagrange(n) => k_lagrange(src, idx, x, *n),
                        Cand::BSpline => k_bspline(src, idx, x),
                        _ => unreachable!(),
                    };
                }
            }
            t += rate as f64;
        }
    }
}

/// 2x upsample with a 31-tap Kaiser halfband. Doubles the memory footprint of
/// every loaded sample, which is the real objection to this option on a deck
/// holding multi-minute tracks.
fn halfband_upsample_2x(src: &[f32]) -> Vec<f32> {
    let tab = SincTable::new(16, 8.0, 1.0);
    let mut out = vec![0.0f32; src.len() * 2];
    for (i, o) in out.iter_mut().enumerate() {
        let t = i as f64 / 2.0;
        *o = if i % 2 == 0 { src.get(i / 2).copied().unwrap_or(0.0) } else { tab.sample(src, t, 1.0) };
    }
    out
}

// ---------------------------------------------------------------------------

/// Long-period fractional rates only — the ones a tempo-synced deck runs at.
/// 1.0 / 2.0 / 1.5 / 0.5 are excluded on purpose: they visit one or two
/// fractional phases and report a kernel far cleaner than it is.
const RATES: &[(f32, &str)] = &[
    (1.029_302_2, "+2.5% tempo"),
    (0.793_700_5, "-4 semitones"),
    (1.059_463_1, "+1 semitone"),
];
const FREQS: &[f32] = &[997.0, 5_000.0, 10_000.0];

/// Offset far enough in that a wide kernel is never clamped at the buffer head.
const HEAD: usize = 128;

fn measure(c: &Cand, freq: f32, rate: f32) -> (f32, f32) {
    let need = WARM + FFT;
    let frames = (need as f32 * rate).ceil() as usize + 4 * 64 + 2 * HEAD;
    let src: Vec<f32> = (0..frames).map(|i| tone_sample(i, freq, SR, AMP)).collect();
    // Render from HEAD so `t` starts clear of the buffer edge.
    let mut out = vec![0.0f32; need];
    c.render(&src, rate, &mut out, HEAD as f64);

    let tail = &out[WARM..];
    let f_out = freq * rate;
    let t = thd_n(tail, f_out, SR, FFT);
    let lvl = level_db_at(&spectrum(tail, FFT), f_out, SR, FFT);
    let refr: Vec<f32> = (0..FFT).map(|i| tone_sample(i, f_out, SR, AMP)).collect();
    let rl = level_db_at(&spectrum(&refr, FFT), f_out, SR, FFT);
    (t, lvl - rl)
}

/// Wall time per output sample, measured on the real render loop.
fn cost_ns(c: &Cand, rate: f32) -> f64 {
    let n = 200_000usize;
    let frames = (n as f32 * rate).ceil() as usize + 4096;
    let src: Vec<f32> = (0..frames).map(|i| tone_sample(i, 997.0, SR, AMP)).collect();
    let mut out = vec![0.0f32; n];
    // Warm caches / branch predictors, and for Oversample2x pay the upsample
    // once outside the timed region is NOT possible — it is part of the cost.
    c.render(&src, rate, &mut out, HEAD as f64);
    let t0 = Instant::now();
    c.render(&src, rate, &mut out, HEAD as f64);
    let el = t0.elapsed().as_secs_f64();
    std::hint::black_box(&out);
    el / n as f64 * 1e9
}

fn main() {
    let cands = vec![
        Cand::Linear,
        Cand::Catmull,
        Cand::Lagrange(4),
        Cand::Lagrange(6),
        Cand::BSpline,
        Cand::Oversample2x,
        Cand::Sinc(SincTable::new(8, 7.0, 0.90)),
        Cand::Sinc(SincTable::new(16, 9.0, 0.90)),
        Cand::Sinc(SincTable::new(32, 10.0, 0.92)),
        Cand::Sinc(SincTable::new(64, 12.0, 0.94)),
    ];

    let floor = audio_dsp::measurement::analyser_floor(997.0, SR, FFT);
    println!("Analyser floor {:.7}% ({:.1} dB). Rates are long-period only.\n",
        floor * 100.0, db(floor));

    println!("=== THD+N at +2.5% tempo, by source frequency ===");
    println!("  {:<30} {:>11} {:>11} {:>11}  {:>9}", "kernel", "997 Hz", "5 kHz", "10 kHz", "ns/sample");
    for c in &cands {
        let mut row = String::new();
        for f in FREQS {
            let (t, _) = measure(c, *f, RATES[0].0);
            row += &format!(" {:>10.1}", db(t));
        }
        println!("  {:<30}{row}  {:>9.1}", c.name(), cost_ns(c, RATES[0].0));
    }

    println!("\n=== passband level error at 10 kHz (droop) ===");
    for c in &cands {
        let (_, g) = measure(c, 10_000.0, RATES[0].0);
        println!("  {:<30} {:+.3} dB", c.name(), g);
    }

    println!("\n=== rate dependence, 5 kHz source ===");
    print!("  {:<30}", "kernel");
    for (_, n) in RATES { print!(" {:>14}", n); }
    println!();
    for c in &cands {
        print!("  {:<30}", c.name());
        for (r, _) in RATES {
            let (t, _) = measure(c, 5_000.0, *r);
            print!(" {:>13.1}", db(t));
        }
        println!();
    }

    println!("\n=== decimation aliasing: 16 kHz source, rate > 1 ===");
    println!("  Only a kernel that low-passes with the rate can fix this.");
    println!("  {:<30} {:>12} {:>12} {:>12}", "kernel", "r=1.6", "r=1.8", "r=2.2");
    for c in &cands {
        print!("  {:<30}", c.name());
        for rate in [1.6f32, 1.8, 2.2] {
            let f_src = 16_000.0f32;
            let need = WARM + FFT;
            let frames = (need as f32 * rate).ceil() as usize + 4 * 64 + 2 * HEAD;
            let src: Vec<f32> = (0..frames).map(|i| tone_sample(i, f_src, SR, AMP)).collect();
            let mut out = vec![0.0f32; need];
            c.render(&src, rate, &mut out, HEAD as f64);
            let mags = spectrum(&out[WARM..], FFT);
            let refr: Vec<f32> = (0..FFT).map(|i| tone_sample(i, f_src, SR, AMP)).collect();
            let rl = level_db_at(&spectrum(&refr, FFT), f_src, SR, FFT);
            // The fold, wherever it landed: loudest bin excluding DC.
            let (pk, _) = mags.iter().enumerate().skip(8)
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap();
            let pk_hz = pk as f32 * SR / FFT as f32;
            let lvl = level_db_at(&mags, pk_hz, SR, FFT);
            print!(" {:>7.0}Hz{:>+5.0}", pk_hz, lvl - rl);
        }
        println!();
    }

    println!("\n=== support (source samples touched per output sample) ===");
    for c in &cands {
        println!("  {:<30} rate 1.0: {:>5.1}   rate 2.0: {:>5.1}",
            c.name(), c.support(1.0), c.support(2.0));
    }
}
