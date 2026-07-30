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
//! | 997 Hz | -91.8 dB | -94.9 dB |
//! | 5 kHz | -48.9 dB | -95.0 dB |
//! | 10 kHz | **-29.0 dB** | **-92.8 dB** |
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
//! # Why 16 taps
//!
//! Measured knee of the cost/quality curve (`probe_resampler_candidates`),
//! ns per output sample in a scalar reference implementation:
//!
//! | taps | 10 kHz THD+N | cost |
//! | ---: | ---: | ---: |
//! | 8 | -73.6 dB | 104 ns |
//! | **16** | **-92.8 dB** | **196 ns** |
//! | 32 | -101.2 dB | 356 ns |
//! | 64 | -99.1 dB | 675 ns |
//!
//! 16 taps buys 19 dB over 8 for 92 ns. 32 buys a further 8 dB for 160 ns more,
//! on the hottest node in the graph. 64 is WORSE than 32 — past that point the
//! table's own linear interpolation between entries becomes the floor, so wider
//! kernels stop helping and only cost. -92.8 dB is 33 dB below the audible
//! threshold and comparable to good converter hardware.

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

/// Kaiser shape parameter. ~9.0 gives sidelobes around -100 dB, which is where
/// the 16-tap kernel's own limit sits; more window suppression would only trade
/// stopband for a wider transition.
const BETA: f64 = 9.0;

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
    h: Vec<f32>,
}

impl SincTable {
    fn build() -> Self {
        let half = TAPS as f64 / 2.0;
        let n = TAPS * RES + 1;
        let mut h = Vec::with_capacity(n);
        let i0_beta = bessel_i0(BETA);
        for i in 0..n {
            let u = -half + i as f64 / RES as f64;
            let a = std::f64::consts::PI * FC * u;
            let sinc = if a.abs() < 1e-12 { 1.0 } else { a.sin() / a };
            let r = (u / half).clamp(-1.0, 1.0);
            let w = bessel_i0(BETA * (1.0 - r * r).max(0.0).sqrt()) / i0_beta;
            h.push((FC * sinc * w) as f32);
        }
        Self { h }
    }

    /// Kernel value at `u`, linearly interpolated between table entries.
    #[inline(always)]
    fn at(&self, u: f32) -> f32 {
        let pos = (u + TAPS as f32 * 0.5) * RES as f32;
        if pos < 0.0 { return 0.0; }
        let i = pos as usize;
        // `h` carries one extra entry so `i + 1` is in range for every valid pos.
        if i + 1 >= self.h.len() { return 0.0; }
        let f = pos - i as f32;
        let a = self.h[i];
        a + (self.h[i + 1] - a) * f
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
        let half = (TAPS as f32 * 0.5 * s) as f64;
        let lo = (t - half).ceil();
        let hi = (t + half).floor();
        if !hi.is_finite() || hi < 0.0 { return 0.0; }
        let lo_i = if lo < 0.0 { 0usize } else { lo as usize };
        let hi_i = (hi as usize).min(buf.len() - 1);
        if lo_i > hi_i { return 0.0; }

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
