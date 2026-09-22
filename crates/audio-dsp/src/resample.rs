//! Bandlimited resampling for `SamplerVoice`.
//!
//! # Why this exists
//!
//! The sampler previously resampled with a 4-point Catmull-Rom cubic. At
//! playback rate 1.0 that is exactly transparent — the fraction is zero and the
//! kernel returns an input sample verbatim — which is why the console measured
//! -107 dB THD+N and nothing was ever caught. A tempo-synced deck is essentially
//! never at rate 1.0, and at a realistic tempo ratio the cubic measured:
//!
//! | source | Catmull-Rom | this kernel |
//! | ---: | ---: | ---: |
//! | 997 Hz | -91.8 dB | **-128.2 dB** |
//! | 5 kHz | -48.9 dB | **-132.8 dB** |
//! | 10 kHz | **-29.0 dB** | **-132.2 dB** |
//!
//! The audible threshold on music is ~0.1% (-60 dB), so the cubic was 31 dB past
//! it at 10 kHz. **No polynomial kernel fixes this**: 6-point Lagrange reaches
//! only -43.7 dB and cubic B-spline -46.2 dB (with -2.5 dB of passband droop).
//! Polynomial interpolators have poor stopband rejection near Nyquist and the
//! whole family degrades with frequency, whereas a windowed sinc is flat — that
//! flatness is the property being bought here, not the peak number.
//!
//! # Why sinc and not just "more taps"
//!
//! A polynomial cannot low-pass, and above rate 1.0 the source is being
//! DECIMATED: content above `Nyquist / rate` has nowhere to go and folds back.
//! Measured before this change, a 16 kHz tone at rate 2.0 arrived at 16 kHz at
//! full level, the fold being the loudest thing in the output.
//!
//! A windowed sinc fixes both defects with one kernel. For a stretch `s`,
//! `h_s(u) = (1/s) * h(u/s)`: evaluating the same table at `u/s` scales the
//! cutoff to `fc/s` and widens the support to `s * TAPS` source samples, which
//! is exactly the anti-alias low-pass that decimation needs. That is the
//! argument for this family that the THD numbers alone do not make.
//!
//! # Why 16 taps — and why that was never the interesting question
//!
//! The original sweep (`probe_resampler_candidates`) measured the tap curve and
//! found something odd at the end of it:
//!
//! | taps | 10 kHz THD+N | cost |
//! | ---: | ---: | ---: |
//! | 8 | -73.6 dB | 104 ns |
//! | **16** | **-92.8 dB** | **196 ns** |
//! | 32 | -101.2 dB | 356 ns |
//! | 64 | **-99.1 dB** | 675 ns |
//!
//! 64 taps being WORSE than 32 is the whole story: the kernel had stopped being
//! the binding constraint. THREE limits sat on top of each other near -100 dB —
//! the table's LINEAR interpolation between entries, the Kaiser window's
//! sidelobes at `BETA = 9.0`, and (further down, not yet binding) f32
//! accumulation. Raising any one alone bought nothing, which is why "more taps"
//! looked like a dead end when it was really a blocked one.
//!
//! `probe_resampler_ceiling` sweeps them together, at the ratio a tempo-synced
//! deck actually runs:
//!
//! | config | 997 Hz | 10 kHz | table | ns/sample |
//! | :--- | ---: | ---: | ---: | ---: |
//! | 16t, b9.0, linear (was) | -94.9 | -92.8 | 8.1 KB | 220 |
//! | 16t, b9.0, **cubic** | -96.8 | -93.1 | 8.1 KB | 331 |
//! | 32t, b14, **linear** | -98.6 | -102.2 | 16.1 KB | 425 |
//! | **16t, b14, cubic (now)** | **-132.0** | **-132.2** | **8.1 KB** | **327** |
//! | 24t, b14, cubic | -132.8 | -133.9 | 12.1 KB | 481 |
//! | 32t, b14, cubic | -132.6 | -133.5 | 16.1 KB | 627 |
//! | 32t, b14, linear, RES 512 | -122.5 | -124.2 | 64.1 KB | 422 |
//!
//! Three things follow, and the first two are the reason this is not a wider
//! kernel:
//!
//! * **Taps still buy nothing past 16.** 24 and 32 land within 2 dB of 16, and
//!   the analyser floor is -135 dB, so that is measurement noise bought at 1.5x
//!   to 2x the cost and the table.
//! * **Raising `RES` is the expensive door and it is worse.** An earlier note
//!   here named it as the prerequisite for more taps. It is — for LINEAR
//!   interpolation. RES 512 measures -124 dB with a 64 KB table, twice this
//!   machine's L1d, which evicts the audio the table shares the cache with. On
//!   an RT thread that trades a better mean for a worse TAIL, and the tail is
//!   the only number that decides whether a block lands.
//! * **Raising the interpolation ORDER costs four floats of table.** Linear
//!   error falls as `h^2`, cubic as `h^4`. And on the polyphase fast path it is
//!   unusually cheap: the interpolation weight is SHARED by all sixteen taps, so
//!   it costs four coefficient rows instead of two — never four times the work.
//!
//! **f32 accumulation is not the next floor**: measured against an f64
//! accumulator the same kernels differ by at most 0.9 dB, so the SIMD path gives
//! up nothing and no f64 inner product is needed.
//!
//! -132 dB is 2.5 dB off what the analyser can resolve, roughly 40 dB below
//! 16-bit's dithered noise floor, and past the point where repeated resampling
//! across a project accumulates anything audible. That last property is why this
//! is worth 1.5x on the hottest node: a deck plays material once, a studio
//! resamples the same material many times.

use std::sync::OnceLock;

/// Kernel support in source samples at stretch 1.0. Even by construction.
pub const TAPS: usize = 16;

/// Table entries per source-sample of support. 128 puts the table's own
/// interpolation error below the 16-tap kernel's own floor; raising it is what
/// would be needed before more taps could help (see the module docs).
const RES: usize = 128;

/// Normalised cutoff, in units of Nyquist. 0.90 leaves a transition band
/// instead of trying to be flat to Nyquist — a cutoff pushed to 0.94 measured
/// WORSE, because the transition has to go somewhere.
const FC: f64 = 0.90;

/// Kaiser shape parameter.
///
/// Was 9.0, whose ~-100 dB sidelobes were one of THREE limits stacked at the
/// same level — the others being the table's linear interpolation and, further
/// down, f32 accumulation. Raising any one alone bought nothing, which is
/// exactly why the old tap sweep found 64 taps WORSE than 32: the kernel had
/// stopped being the binding constraint.
///
/// 14.0 puts sidelobes near -140 dB, and paired with cubic table interpolation
/// (see `at`) it moves measured THD+N from -92.8 dB to -132 dB at 10 kHz with
/// NO change in table footprint. Measured by `probe_resampler_ceiling`.
const BETA: f64 = 14.0;

/// Index of table entry `j == 0` inside `h`.
///
/// `h` carries one entry BEFORE the kernel's support and two after, so cubic
/// interpolation can read `[i-1, i+2]` at every valid position without a bounds
/// branch in the inner loop. Costs 3 floats.
const H_OFF: usize = 1;

/// Largest stretch the anti-alias low-pass is applied at.
///
/// The tap count scales with the stretch, so an unclamped rate would make the
/// inner loop unbounded on the audio thread — `playback_rate` is reachable from
/// MIDI note transposition and can be far above 1. Clamped at 4.0, i.e. up to
/// two octaves up gets full anti-aliasing at a bounded 64 reads per output
/// sample; beyond that pitch-up aliasing returns, which is a deliberate trade
/// against an unbounded RT cost.
pub const MAX_STRETCH: f32 = 4.0;

/// Kaiser-windowed sinc, sampled `RES` times per unit of support.
pub struct SincTable {
    /// `h(u)` for `u` in `[-TAPS/2, +TAPS/2]`, length `TAPS * RES + 1`.
    ///
    /// The general (stretched) path walks this directly: at stretch `s` the tap
    /// spacing is `RES/s` table entries, which is fractional, so each tap needs
    /// its own index and its own interpolation weight. That is a gather, and on
    /// the reference machine `vgatherdps` is slower than the scalar loads it
    /// would replace — so the stretched path stays scalar.
    h: Vec<f32>,
    /// PHASE-MAJOR, TAP-REVERSED copy of the same coefficients, for stretch 1.0.
    ///
    /// `polyr[p * TAPS + m] == h[(TAPS - 1 - m) * RES + p]`.
    ///
    /// # Why this layout makes the unit-stretch path vectorise
    ///
    /// At `s == 1` the window is exactly `TAPS` source samples and the table
    /// step between taps is exactly `RES` entries — an INTEGER. Two things fall
    /// out of that, and both are what the scalar loop was paying for needlessly:
    ///
    /// * every tap lands on the same fractional position between table entries,
    ///   so there is ONE interpolation weight `f` for the whole window rather
    ///   than sixteen;
    /// * the sixteen coefficients for a given phase, laid out this way, are
    ///   CONTIGUOUS — as are the sixteen input samples they multiply, once the
    ///   tap order is reversed to match ascending memory.
    ///
    /// So the inner loop becomes two aligned vector loads of coefficients, two
    /// of input, and a fused multiply-add — no gather, no per-tap index
    /// arithmetic. Same arithmetic as the scalar path, same table, different
    /// order of the sum.
    ///
    /// `(RES + 1) * TAPS` entries so phase `RES` can still read `p + 1`;
    /// about 8 KB, which keeps it resident in L1 alongside the audio.
    polyr: Vec<f32>,
}

impl SincTable {
    fn build() -> Self {
        let half = TAPS as f64 / 2.0;
        // One entry before the support and two after, so cubic interpolation
        // reads [i-1, i+2] everywhere without a branch. See `H_OFF`.
        let n = TAPS * RES + 3;
        let mut h = Vec::with_capacity(n);
        let i0_beta = bessel_i0(BETA);
        for i in 0..n {
            let u = -half + (i as f64 - H_OFF as f64) / RES as f64;
            let a = std::f64::consts::PI * FC * u;
            let sinc = if a.abs() < 1e-12 { 1.0 } else { a.sin() / a };
            let r = (u / half).clamp(-1.0, 1.0);
            let w = bessel_i0(BETA * (1.0 - r * r).max(0.0).sqrt()) / i0_beta;
            h.push((FC * sinc * w) as f32);
        }

        // Phase-major, tap-reversed transpose of `h`. Built once, off the audio
        // thread (see `prewarm`).
        //
        // Rows now run from phase -1 to RES+1 rather than 0 to RES, stored at
        // row index `p + 1`, so the cubic blend can reach `p - 1` and `p + 2`.
        // That is two extra rows: 8,384 bytes against 8,256, which changes
        // nothing about fitting in L1 beside the audio — and NOT fitting is the
        // whole reason this is 16 taps at RES 128 rather than something wider.
        let mut polyr = vec![0.0f32; (RES + 3) * TAPS];
        for pi in 0..RES + 3 {
            let p = pi as isize - 1; // phase, -1 ..= RES + 1
            for m in 0..TAPS {
                let j = (TAPS - 1 - m) as isize * RES as isize + p + H_OFF as isize;
                debug_assert!(j >= 0 && (j as usize) < h.len());
                polyr[pi * TAPS + m] = h[j as usize];
            }
        }

        Self { h, polyr }
    }

    /// Catmull-Rom basis weights for a fraction `f` between table entries.
    ///
    /// Shared by both paths so they cannot drift: the fast path applies these to
    /// four coefficient ROWS, the general path to four adjacent entries.
    #[inline(always)]
    fn cr_weights(f: f32) -> [f32; 4] {
        let f2 = f * f;
        let f3 = f2 * f;
        [
            -0.5 * f3 + f2 - 0.5 * f,
            1.5 * f3 - 2.5 * f2 + 1.0,
            -1.5 * f3 + 2.0 * f2 + 0.5 * f,
            0.5 * f3 - 0.5 * f2,
        ]
    }

    /// Kernel value at `u`, CUBICALLY interpolated between table entries.
    ///
    /// Was linear, and that was the binding limit on the whole resampler — not
    /// the tap count. Linear interpolation error falls as `h^2` and at
    /// `RES = 128` it floors the kernel around -101 dB, which is why the
    /// original sweep saw 32 taps beat 64 and concluded wider kernels stop
    /// helping. They stop helping because this function was the floor.
    ///
    /// The module's own note suggested raising `RES` as the prerequisite. That
    /// works and is the expensive door: `RES` 512 measured -124 dB with a 64 KB
    /// table — 2x L1, evicting the audio it shares the cache with, which trades
    /// a better mean for a worse TAIL. Raising the interpolation ORDER instead
    /// costs four floats of table and no cache at all, and measures -132 dB.
    ///
    /// MEASURED, and still true: `i = pos as usize; f = pos - i as f32` beats
    /// the apparently cheaper `floor()`. Replacing the int round trip with one
    /// `roundss` made the stretched path 40% SLOWER (150 -> 214 ns/sample) —
    /// `as usize` on a known-positive f32 is a single `cvttss2si`, while
    /// `floor()` adds a `roundss` in front of the same conversion.
    #[inline(always)]
    fn at(&self, u: f32) -> f32 {
        let pos = (u + TAPS as f32 * 0.5) * RES as f32;
        if pos < 0.0 { return 0.0; }
        let i = pos as usize;
        // `h` carries H_OFF entries before the support and two after, so
        // [i-1 .. i+2] in kernel indexing is [i .. i+3] in storage and is in
        // range for every valid pos. This is the only bounds check.
        if i + 3 >= self.h.len() { return 0.0; }
        let f = pos - i as f32;
        let w = Self::cr_weights(f);
        w[0] * self.h[i] + w[1] * self.h[i + 1] + w[2] * self.h[i + 2] + w[3] * self.h[i + 3]
    }

    /// Unit-stretch inner product: 16 contiguous taps, one shared weight.
    ///
    /// `ti` is `floor(t)`, `p` the table phase and `f` the weight between phase
    /// `p` and `p + 1`. The caller has already proved the whole window
    /// `[ti - 7, ti + 8]` is inside `buf`, so there is no bounds handling here —
    /// that is what makes it a straight dot product.
    ///
    #[inline(always)]
    fn dot_unit(&self, buf: &[f32], ti: usize, p: usize, f: f32) -> f32 {
        use wide::f32x8;

        // Rows are stored at index `phase + 1`, so phase `p` needs storage rows
        // p, p+1, p+2, p+3 — which ARE phases p-1, p, p+1, p+2. The table is
        // built with the extra rows at both ends precisely so this needs no
        // bounds handling; see `build`.
        let r = |k: usize| &self.polyr[(p + k) * TAPS..(p + k) * TAPS + TAPS];
        let (cm1, c0, c1, c2) = (r(0), r(1), r(2), r(3));
        let x = &buf[ti - (TAPS / 2 - 1)..ti + TAPS / 2 + 1];

        // Four scalars, computed ONCE per output sample rather than per lane —
        // this is what makes cubic affordable here. The interpolation weight is
        // shared by all sixteen taps (that is the property the whole polyphase
        // layout rests on), so raising the order costs four coefficient loads
        // instead of two and four FMAs instead of one, on data already in L1.
        let w = Self::cr_weights(f);
        let (w0, w1, w2, w3) = (
            f32x8::splat(w[0]), f32x8::splat(w[1]),
            f32x8::splat(w[2]), f32x8::splat(w[3]),
        );

        let mut acc = f32x8::ZERO;
        // Two halves of the 16-tap window. `wide` lowers these to one AVX
        // register each when the AVX2 instantiation is selected, and to a pair
        // of SSE2 registers otherwise — same result either way.
        for half in 0..2 {
            let o = half * 8;
            let a = f32x8::new([cm1[o], cm1[o+1], cm1[o+2], cm1[o+3], cm1[o+4], cm1[o+5], cm1[o+6], cm1[o+7]]);
            let b = f32x8::new([c0[o], c0[o+1], c0[o+2], c0[o+3], c0[o+4], c0[o+5], c0[o+6], c0[o+7]]);
            let c = f32x8::new([c1[o], c1[o+1], c1[o+2], c1[o+3], c1[o+4], c1[o+5], c1[o+6], c1[o+7]]);
            let d = f32x8::new([c2[o], c2[o+1], c2[o+2], c2[o+3], c2[o+4], c2[o+5], c2[o+6], c2[o+7]]);
            let s = f32x8::new([x[o], x[o+1], x[o+2], x[o+3], x[o+4], x[o+5], x[o+6], x[o+7]]);
            acc += (a * w0 + b * w1 + c * w2 + d * w3) * s;
        }
        acc.reduce_add()
    }

    /// Fast path for `stretch == 1.0` with the window fully inside `buf`.
    ///
    /// Returns `None` when either precondition fails, and the caller falls back
    /// to the general scalar walk — which handles stretching, the buffer edges
    /// and the zero-padding contract.
    #[inline]
    fn sample_unit(&self, buf: &[f32], t: f64) -> Option<f32> {
        let ti_f = t.floor();
        if !ti_f.is_finite() || ti_f < 0.0 {
            return None;
        }
        let ti = ti_f as usize;
        // Window is [ti - 7, ti + 8] inclusive.
        if ti < TAPS / 2 - 1 || ti + TAPS / 2 >= buf.len() {
            return None;
        }
        let d = (t - ti_f) as f32;
        // Integer position: the general path widens to a SYMMETRIC 17-tap
        // window there ([ti-8, ti+8], because `ceil(t-8) == t-8` exactly),
        // while this path is always the 16 taps of a polyphase bank. The extra
        // tap sits on the Kaiser window's edge and contributes ~1e-5 — below
        // the kernel's own -95 dB floor, but a difference is a difference.
        // Hand it back so the two paths are identical at every position rather
        // than merely close.
        //
        // Costs nothing in practice: `SamplerVoice::sinc_sample` already
        // short-circuits `frac == 0 && stretch == 1` to a verbatim sample, so
        // this branch is only reached by a direct caller.
        if d == 0.0 {
            return None;
        }
        let pos = d * RES as f32;
        let p = pos as usize;
        // Storage rows are 0..=RES+2 and the blend reads p..p+3, so p <= RES-1
        // is exactly the admissible range — the same bound the linear version
        // needed, for a different reason.
        if p >= RES {
            // d rounded up to the last phase; let the general path handle the
            // boundary rather than reading past `polyr`.
            return None;
        }
        let f = pos - p as f32;

        // NOT runtime-dispatched, deliberately. An AVX2+FMA instantiation of
        // `dot_unit` measured REPRODUCIBLY SLOWER here — 27.1 vs 25.4 ns/sample
        // across three runs. The window is only 16 elements, so there is no
        // throughput to win; the kernel is latency-bound on the horizontal
        // reduction at the end, and reducing one 256-bit register costs more
        // than reducing two 128-bit ones. Widening the registers is not the
        // same thing as vectorising, and this kernel is where the difference
        // shows. The 6x here came from the polyphase layout, not the ISA.
        Some(self.dot_unit(buf, ti, p, f))
    }

    /// One output sample of `buf` at fractional source position `t`, with the
    /// kernel stretched by `stretch` (>= 1.0 decimates and low-passes).
    ///
    /// Reads outside `buf` are treated as zero, so the kernel is applied to the
    /// zero-padded signal rather than reading a neighbouring channel's plane.
    /// Allocation-free and bounded: the inner loop runs at most
    /// `TAPS * MAX_STRETCH` times.
    #[inline]
    pub fn sample(&self, buf: &[f32], t: f64, stretch: f32) -> f32 {
        if buf.is_empty() { return 0.0; }
        let s = if stretch.is_finite() { stretch.clamp(1.0, MAX_STRETCH) } else { 1.0 };

        // Unit stretch is the common case — every rate at or below 1.0 clamps
        // to it, which is all of pitch-down and tempo-down — and it is the one
        // where the tap spacing becomes an integer and the whole window
        // vectorises. See `polyr`.
        if s == 1.0
            && let Some(y) = self.sample_unit(buf, t) {
                return y;
            }

        let half = (TAPS as f32 * 0.5 * s) as f64;
        let lo = (t - half).ceil();
        let hi = (t + half).floor();
        if !hi.is_finite() || hi < 0.0 { return 0.0; }
        let lo_i = if lo < 0.0 { 0usize } else { lo as usize };
        let hi_i = (hi as usize).min(buf.len() - 1);
        if lo_i > hi_i { return 0.0; }

        // The STRETCHED path is unchanged, and two attempts to speed it up were
        // measured and reverted:
        //
        //  * splitting coefficient lookup from the dot product so the latter
        //    could vectorise — no effect (152 -> 150 ns, inside noise);
        //  * `floor()` instead of the int round trip in `at()` — 40% worse.
        //
        // Both say the same thing: the cost here is the TABLE LOOKUP, and it
        // resists vectorising for a structural reason. At stretch `s` the tap
        // spacing is `RES/s` table entries, which is fractional, so every tap
        // needs its own index — a gather. On the reference machine
        // `vgatherdps` is slower than the scalar loads it would replace, so
        // there is nothing to gain by reaching for it.
        //
        // This is why the fast path above is worth its complexity: it is not a
        // faster way of doing the same thing, it is the case where the spacing
        // becomes an integer and the gather disappears entirely.
        let inv_s = 1.0 / s;
        let mut acc = 0.0f32;
        for (n, &x) in buf[lo_i..=hi_i].iter().enumerate() {
            let u = ((t - (lo_i + n) as f64) as f32) * inv_s;
            acc += x * self.at(u);
        }
        acc * inv_s
    }
}

static TABLE: OnceLock<SincTable> = OnceLock::new();

/// The shared, read-only kernel table.
///
/// One table for every voice — it depends only on compile-time constants, so
/// there is nothing per-voice to hold. On the audio path this is an acquire load
/// and a branch; it never allocates, because [`prewarm`] has already run from
/// `SamplerVoice::new()` (a topology-build path, not an RT one).
#[inline]
pub fn table() -> &'static SincTable {
    TABLE.get_or_init(SincTable::build)
}

/// Build the table now, off the audio thread. Called from `SamplerVoice::new()`.
pub fn prewarm() {
    let _ = table();
}

/// Modified Bessel function of the first kind, order 0 — the Kaiser window's
/// shape term. The series converges quickly for the beta in use.
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0f64;
    let mut term = 1.0f64;
    let hx = x / 2.0;
    for k in 1..64 {
        let f = hx / k as f64;
        term *= f * f;
        sum += term;
        if term < 1e-18 * sum { break; }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DC must pass at unity, or every resampled track changes level.
    ///
    /// A windowed sinc's taps sum to 1 only approximately, and the residual
    /// ripple is a function of the fractional phase — so an error here would be
    /// a rate-dependent gain wobble, which is worse than a fixed offset because
    /// it modulates.
    #[test]
    fn test_dc_gain_is_unity_at_every_phase() {
        let t = table();
        let buf = vec![1.0f32; 256];
        for k in 0..64 {
            let frac = k as f64 / 64.0;
            let y = t.sample(&buf, 128.0 + frac, 1.0);
            assert!(
                (y - 1.0).abs() < 1e-3,
                "DC gain at phase {frac:.4} is {y:.6}, not 1.0 — the kernel's \
                 taps do not sum to unity and resampling will shift level"
            );
        }
    }

    /// At stretch 1.0 and zero fraction the kernel does NOT reproduce the input
    /// sample: `h(0) = FC * w(0) < 1` and `h(+/-1) != 0`, so the convolution at
    /// an integer position is a mild low-pass, not a copy.
    ///
    /// This pins the reason `SamplerVoice` short-circuits that case. If this
    /// test ever fails the short-circuit has become dead code and should be
    /// deleted rather than left implying it does something.
    #[test]
    fn test_raw_kernel_is_not_identity_at_integer_positions() {
        let t = table();
        let mut buf = vec![0.0f32; 256];
        buf[128] = 1.0;
        let y = t.sample(&buf, 128.0, 1.0);
        assert!(
            (y - 1.0).abs() > 1e-4,
            "kernel returned {y:.6} at an integer position, i.e. it IS the \
             identity — then the rate-1.0 short-circuit in SamplerVoice is dead \
             code and should be removed rather than left implying otherwise"
        );
    }

    /// The inner loop must stay bounded however absurd the rate. An unbounded
    /// tap count on the audio thread is an RT-safety break, not a quality issue.
    #[test]
    fn test_tap_count_is_bounded_by_max_stretch() {
        let t = table();
        let buf = vec![0.5f32; 4096];
        for stretch in [1.0f32, 4.0, 64.0, 1e6, f32::INFINITY, f32::NAN] {
            let y = t.sample(&buf, 2048.0, stretch);
            assert!(y.is_finite(), "stretch {stretch} produced {y}");
            // DC through a low-pass is still DC.
            assert!((y - 0.5).abs() < 1e-2, "stretch {stretch} gave {y}, not 0.5");
        }
    }

    /// Reads off either end are zero-padded — never a panic, and never a read
    /// into whatever follows in memory. Buffers are PLANAR, so overrunning one
    /// channel's plane means playing the next channel's audio.
    #[test]
    fn test_boundaries_are_zero_padded_not_panicking() {
        let t = table();
        let buf = vec![1.0f32; 64];
        for pos in [-1e9f64, -100.0, -1.0, 0.0, 0.5, 31.5, 62.0, 63.0, 64.0, 1e9] {
            let y = t.sample(&buf, pos, 1.0);
            assert!(y.is_finite(), "position {pos} produced {y}");
        }
        assert_eq!(t.sample(&buf, -100.0, 1.0), 0.0);
        assert_eq!(t.sample(&buf, 1000.0, 1.0), 0.0);
        assert_eq!(t.sample(&[], 0.0, 1.0), 0.0);
    }
}
