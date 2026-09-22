//! Runtime SIMD dispatch: portable binary, full width where the CPU allows.
//!
//! # Why not just raise `target-cpu`
//!
//! Without any flags, cargo targets the bare `x86-64` baseline — SSE2, a 2003
//! instruction set. Every `wide::f32x8` in this crate then lowers to TWO SSE2
//! 128-bit registers instead of one AVX register, and the filter inner loops get
//! separate multiply and add instead of FMA. Measured on the 8-channel biquad,
//! that costs about 1.3x.
//!
//! Setting `-C target-cpu=x86-64-v3` fixes the speed and breaks the binary: it
//! emits AVX2 unconditionally, so anything older than Haswell (2013) / Excavator
//! (2015) dies with SIGILL rather than running slower. For a distributable
//! workstation that is the wrong trade.
//!
//! # How this works
//!
//! Each hot kernel is written ONCE as an `#[inline(always)]` body, then
//! instantiated twice: a baseline copy, and a copy inside a wrapper carrying
//! `#[target_feature(enable = "avx,avx2,fma")]`. LLVM compiles the inlined body
//! with those features enabled, so the same `wide::f32x8` source becomes real
//! 256-bit AVX with FMA in that instantiation. [`level`] picks between them on
//! one relaxed atomic load and a branch.
//!
//! This is the same mechanism the `multiversion` crate automates; done by hand
//! here because it applies to a handful of functions and a hand-rolled version
//! is one file instead of a proc-macro dependency in the DSP crate.
//!
//! # RT-safety
//!
//! `is_x86_feature_detected!` runs CPUID on first use and caches it. That must
//! not happen on the audio thread, so [`prewarm`] resolves it during setup —
//! same pattern as the resampler's sinc table. After that, [`level`] is a
//! relaxed load of a `u8`.
//!
//! # Correctness
//!
//! Both instantiations come from the same source, so they compute the same
//! thing — but not necessarily bit-identically: FMA does one rounding where
//! multiply-then-add does two. For an IIR filter that difference is fed back, so
//! the two paths can drift by a few ULP over a long block.
//! `tests/dispatch_equivalence_test.rs` pins the difference to a tolerance
//! rather than to bit-equality, and says why.

use std::sync::atomic::{AtomicU8, Ordering};

/// Not yet probed.
const UNPROBED: u8 = 0;
/// Nothing beyond the compilation baseline.
pub const LEVEL_BASELINE: u8 = 1;
/// AVX + AVX2 + FMA available.
pub const LEVEL_AVX2_FMA: u8 = 2;

static LEVEL: AtomicU8 = AtomicU8::new(UNPROBED);

#[cfg(target_arch = "x86_64")]
fn probe() -> u8 {
    // If the build ALREADY targets these (someone set `-C target-cpu=native`),
    // the baseline instantiation is itself AVX2 and the dispatch is a no-op —
    // report the higher level anyway so the reporting is honest.
    if cfg!(all(target_feature = "avx2", target_feature = "fma")) {
        return LEVEL_AVX2_FMA;
    }
    if std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma") {
        LEVEL_AVX2_FMA
    } else {
        LEVEL_BASELINE
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn probe() -> u8 {
    // aarch64's baseline already includes NEON, so `wide` is full width with no
    // dispatch needed. Nothing to select between.
    LEVEL_BASELINE
}

/// The SIMD level this process will use. One relaxed load after [`prewarm`].
#[inline(always)]
pub fn level() -> u8 {
    let cached = LEVEL.load(Ordering::Relaxed);
    if cached != UNPROBED {
        return cached;
    }
    let probed = probe();
    LEVEL.store(probed, Ordering::Relaxed);
    probed
}

/// True when the AVX2+FMA instantiations may be called.
#[inline(always)]
pub fn has_avx2_fma() -> bool {
    level() == LEVEL_AVX2_FMA
}

/// Resolve the CPU probe now, off the audio thread.
///
/// Call from engine setup. Skipping it is not unsound — the first `level()` on
/// the RT thread would simply run CPUID once — but CPUID is a serialising
/// instruction and the audio callback is not where to discover that.
pub fn prewarm() {
    let _ = level();
}

/// Human-readable level, for startup logging and telemetry.
pub fn level_name() -> &'static str {
    match level() {
        LEVEL_AVX2_FMA => "avx2+fma",
        _ => "baseline",
    }
}

/// Force a dispatch level. **Benchmarks and tests only.**
///
/// Two things need this, and neither is served by the probe alone:
///
/// * A/B benchmarking. Comparing the two instantiations across separate BUILDS
///   means comparing across separate machine states — the same binary varied 39%
///   run to run on the reference box, which is wider than the effect being
///   measured. Forcing the level flips paths inside one process, interleaved.
/// * Testing the baseline path on a modern CPU. Without this, every developer
///   machine with AVX2 exercises only the fast instantiation, and the baseline
///   one ships untested.
///
/// Lowering the level is always safe. RAISING it to [`LEVEL_AVX2_FMA`] on a CPU
/// without those features will execute an illegal instruction, so this refuses
/// to do that and returns the level actually in force.
pub fn force_level(requested: u8) -> u8 {
    let ceiling = probe();
    let effective = requested.min(ceiling).max(LEVEL_BASELINE);
    LEVEL.store(effective, Ordering::Relaxed);
    effective
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_is_stable_and_named() {
        prewarm();
        let a = level();
        let b = level();
        assert_eq!(a, b, "the probe must cache");
        assert!(a == LEVEL_BASELINE || a == LEVEL_AVX2_FMA, "unexpected level {a}");
        assert!(!level_name().is_empty());
        eprintln!("audio-dsp SIMD dispatch level: {}", level_name());
    }

    /// On a machine that reports AVX2, the dispatch must actually select it —
    /// otherwise the whole mechanism is inert and nothing would say so.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn selects_avx2_when_the_cpu_has_it() {
        if std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma")
        {
            assert!(has_avx2_fma(), "CPU reports avx2+fma but dispatch chose the baseline path");
        }
    }
}
