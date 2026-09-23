//! **Is the deck isolator's -107 dB THD+N single-precision state, or something else?**
//!
//! `probe_chain_taps` (nullherz-conductor) walks the console stage by stage and
//! finds that everything up to the fx slot measures -135 dB — the analyser's own
//! floor — and the deck isolator adds 27.9 dB in one step, which every stage
//! after it merely passes on. That makes ONE processor the chain's entire
//! distortion budget.
//!
//! The suspicion is numerical, not algorithmic: `DjIsolatorStereo` is eight
//! cascaded `StereoBiquad` stages (Linkwitz-Riley at 300 Hz and 3 kHz, LR4 so
//! two per section) with `wide::f32x4` state in transposed direct form II.
//! 300 Hz at 48 kHz is 0.00625 normalised — poles near z = 1, where the TDF-II
//! recurrence `z1 = x*b1 - y*a1 + z2` cancels catastrophically in f32.
//!
//! This reproduces the SAME cascade twice, changing only the state precision,
//! and measures THD+N of each. Nothing else differs — same coefficients, same
//! topology, same band recombination — so a gap between the two columns is the
//! precision and nothing else.
//!
//! A hypothesis is not a finding until the experiment can come out the other
//! way. If f64 state does NOT close the gap, the cause is elsewhere and the
//! isolator needs a different fix.
//!
//!   cargo run --release -p audio-dsp --example probe_biquad_precision

use std::time::Instant;

use audio_dsp::measurement::{analyser_floor, thd_n, tone_sample};
use audio_dsp::BiquadCoefficients;

const SR: f32 = 48_000.0;
const FFT: usize = 32_768;
const WARM: usize = 8_192;
const AMP: f32 = 0.5;

fn db(x: f32) -> f32 { 20.0 * x.max(1e-20).log10() }

/// Scalar TDF-II with f32 state — one lane of the shipped `StereoBiquad`.
#[derive(Clone, Copy)]
struct Bq32 { c: BiquadCoefficients, z1: f32, z2: f32 }
impl Bq32 {
    fn new(c: BiquadCoefficients) -> Self { Self { c, z1: 0.0, z2: 0.0 } }
    #[inline(always)]
    fn go(&mut self, x: f32) -> f32 {
        let y = x * self.c.b0 + self.z1;
        self.z1 = (x * self.c.b1 - y * self.c.a1) + self.z2;
        self.z2 = x * self.c.b2 - y * self.c.a2;
        y
    }
}

/// The same recurrence with f64 STATE. Coefficients stay as the f32 values the
/// shipped filter designs, so this isolates state precision rather than also
/// improving the coefficients.
#[derive(Clone, Copy)]
struct Bq64 { b0: f64, b1: f64, b2: f64, a1: f64, a2: f64, z1: f64, z2: f64 }
impl Bq64 {
    fn new(c: BiquadCoefficients) -> Self {
        Self { b0: c.b0 as f64, b1: c.b1 as f64, b2: c.b2 as f64,
               a1: c.a1 as f64, a2: c.a2 as f64, z1: 0.0, z2: 0.0 }
    }
    #[inline(always)]
    fn go(&mut self, x: f32) -> f32 {
        let xd = x as f64;
        let y = xd * self.b0 + self.z1;
        self.z1 = (xd * self.b1 - y * self.a1) + self.z2;
        self.z2 = xd * self.b2 - y * self.a2;
        y as f32
    }
}

/// The isolator's three-band split and sum, at unity gain on every band.
macro_rules! isolator {
    ($name:ident, $bq:ty) => {
        struct $name { lp1: $bq, lp2: $bq, hp1: $bq, hp2: $bq, mh1: $bq, mh2: $bq, ml1: $bq, ml2: $bq }
        impl $name {
            fn new(lp: BiquadCoefficients, hp: BiquadCoefficients,
                   mid_hp: BiquadCoefficients, mid_lp: BiquadCoefficients) -> Self {
                Self {
                    lp1: <$bq>::new(lp), lp2: <$bq>::new(lp),
                    hp1: <$bq>::new(hp), hp2: <$bq>::new(hp),
                    mh1: <$bq>::new(mid_hp), mh2: <$bq>::new(mid_hp),
                    ml1: <$bq>::new(mid_lp), ml2: <$bq>::new(mid_lp),
                }
            }
            #[inline(always)]
            fn go(&mut self, x: f32) -> f32 {
                let l = self.lp2.go(self.lp1.go(x));
                let h = self.hp2.go(self.hp1.go(x));
                let m_low = self.mh2.go(self.mh1.go(x));
                let m = self.ml2.go(self.ml1.go(m_low));
                l + m + h
            }
        }
    };
}
isolator!(Iso32, Bq32);
isolator!(Iso64, Bq64);

fn main() {
    // Exactly what DjIsolatorStereo::with_sample_rate builds.
    let lp = BiquadCoefficients::linkwitz_riley_lp(300.0, SR);
    let hp = BiquadCoefficients::linkwitz_riley_hp(3000.0, SR);
    let mid_hp = BiquadCoefficients::linkwitz_riley_hp(300.0, SR);
    let mid_lp = BiquadCoefficients::linkwitz_riley_lp(3000.0, SR);

    println!("Isolator crossover: LR4 at 300 Hz and 3 kHz, 8 biquads, unity on every band.");
    println!("300 Hz / {SR} Hz = {:.5} normalised.\n", 300.0 / SR);
    println!("Low-band (300 Hz) coefficients as designed, f32:");
    println!("  b0 {:+.9e}  b1 {:+.9e}  b2 {:+.9e}", lp.b0, lp.b1, lp.b2);
    println!("  a1 {:+.9e}  a2 {:+.9e}", lp.a1, lp.a2);
    println!("  a1 is within {:.2e} of -2.0 — that is the cancellation.\n", (lp.a1 as f64 + 2.0).abs());

    println!("  {:<10} {:>12} {:>12} {:>10}", "tone", "f32 state", "f64 state", "gain");
    for freq in [110.0f32, 440.0, 997.0, 5_000.0, 10_000.0] {
        let mut a = Iso32::new(lp, hp, mid_hp, mid_lp);
        let mut b = Iso64::new(lp, hp, mid_hp, mid_lp);
        let mut o32 = Vec::with_capacity(FFT);
        let mut o64 = Vec::with_capacity(FFT);
        for i in 0..(WARM + FFT) {
            let x = tone_sample(i, freq, SR, AMP);
            let y32 = a.go(x);
            let y64 = b.go(x);
            if i >= WARM { o32.push(y32); o64.push(y64); }
        }
        let t32 = db(thd_n(&o32, freq, SR, FFT));
        let t64 = db(thd_n(&o64, freq, SR, FFT));
        println!("  {:<10} {:>9.1} dB {:>9.1} dB {:>9.1} dB",
            format!("{freq:.0} Hz"), t32, t64, t64 - t32);
    }

    // What it costs. The shipped state is `wide::f32x4` holding [L, R, 0, 0] —
    // HALF the lanes wasted — so the honest comparison for a stereo filter is
    // f64 in a 128-bit register (two lanes, fully used) against f32 in a
    // 128-bit register with two lanes idle. Same register width either way.
    const N: usize = 4_000_000;
    let src: Vec<f32> = (0..8192).map(|i| tone_sample(i, 997.0, SR, AMP)).collect();
    let mut sink = 0.0f32;

    let mut a = Iso32::new(lp, hp, mid_hp, mid_lp);
    let t0 = Instant::now();
    for i in 0..N { sink += a.go(src[i % src.len()]); }
    let c32 = t0.elapsed().as_nanos() as f64 / N as f64;

    let mut b = Iso64::new(lp, hp, mid_hp, mid_lp);
    let t1 = Instant::now();
    for i in 0..N { sink += b.go(src[i % src.len()]); }
    let c64 = t1.elapsed().as_nanos() as f64 / N as f64;
    std::hint::black_box(sink);

    println!("\n  cost, 8-biquad cascade, scalar reference (one channel):");
    println!("    f32 state {c32:>7.2} ns/sample");
    println!("    f64 state {c64:>7.2} ns/sample   {:.2}x", c64 / c32);

    println!("\n  analyser floor by FFT size (flat = arithmetic, not leakage):");
    for sz in [4_096usize, 8_192, 16_384, 32_768] {
        println!("    {sz:>6} : {:>7.1} dB", db(analyser_floor(997.0, SR, sz)));
    }

    let floor = db(analyser_floor(997.0, SR, FFT));
    println!("\nAnalyser floor {floor:.1} dB — a column at that value is transparent");
    println!("to the limit of measurement, not merely good.");
    println!("\nIf the two columns agree, single precision is NOT the cause and the");
    println!("isolator needs a different explanation.");
}
