//! The dispatched kernels must agree with the baseline ones.
//!
//! Runtime dispatch compiles the same source twice — once at the compilation
//! baseline, once under `#[target_feature(enable = "avx,avx2,fma")]`. Both
//! compute the same thing, but **not bit-identically**: FMA performs one
//! rounding where a separate multiply and add perform two. In a biquad that
//! difference is fed back through `z1`/`z2`, so it compounds across a block
//! rather than staying at one ULP.
//!
//! So these tests pin a TOLERANCE, not equality — and pin it low enough that a
//! real divergence (a transposed coefficient, a lane swapped, state not carried)
//! cannot hide under it. A genuine bug in one instantiation shows up as a
//! difference many orders of magnitude above this floor, not as 1e-6.
//!
//! The tests are only meaningful on a machine that actually has AVX2: on one
//! that does not, both paths ARE the same code and the comparison is trivially
//! true. That is reported rather than silently passing.

use audio_dsp::{dispatch, BiquadCoefficients, SimdBiquad, SummingNode};

/// `force_level` writes a PROCESS-WIDE atomic, and cargo runs the tests in one
/// binary on parallel threads by default. Without this every level-forcing test
/// would race the others and the suite would flake at random. Held for the whole
/// body of any test that forces a level.
static DISPATCH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Take the lock, ignoring poisoning: a panicking test has already failed and
/// should not cascade into every other test in the file.
fn serialize() -> std::sync::MutexGuard<'static, ()> {
    DISPATCH_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Deterministic broadband signal — xorshift, spanning past ±1.
fn signal(n: usize, seed: u32) -> Vec<f32> {
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s as f32 / u32::MAX as f32) * 2.4 - 1.2
        })
        .collect()
}

/// Relative difference, so the tolerance means the same thing at any level.
fn max_rel_diff(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| {
            let d = (x as f64 - y as f64).abs();
            let scale = (x as f64).abs().max((y as f64).abs()).max(1e-3);
            d / scale
        })
        .fold(0.0f64, f64::max)
}

#[test]
fn report_active_level() {
    dispatch::prewarm();
    eprintln!("dispatch level: {}", dispatch::level_name());
    #[cfg(target_arch = "x86_64")]
    if !dispatch::has_avx2_fma() {
        eprintln!(
            "NOTE: this CPU reports no avx2+fma, so the equivalence tests below compare \
             the baseline path against itself and prove nothing about the AVX2 path."
        );
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn biquad_8ch_paths_agree() {
    let _guard = serialize();
    dispatch::prewarm();
    if !dispatch::has_avx2_fma() {
        eprintln!("skipped: no avx2+fma on this CPU");
        return;
    }

    const N: usize = 4096;
    // A resonant low-pass: high feedback, so any state-handling divergence
    // between the two instantiations compounds instead of cancelling.
    let coeffs = BiquadCoefficients::linkwitz_riley_lp(800.0, 44_100.0);

    let ins: Vec<Vec<f32>> = (0..8).map(|c| signal(N, 0x1000 + c as u32)).collect();
    let in_ptrs: [*const f32; 8] = std::array::from_fn(|i| ins[i].as_ptr());

    // Run the SAME kernel through each instantiation, forcing the level. This
    // is the comparison that matters: the two must be the same filter, not just
    // both roughly filter-shaped.
    let run = |level: u8| -> Vec<Vec<f32>> {
        dispatch::force_level(level);
        let mut outs: Vec<Vec<f32>> = (0..8).map(|_| vec![0.0f32; N]).collect();
        let out_ptrs: [*mut f32; 8] = std::array::from_fn(|i| outs[i].as_mut_ptr());
        let mut f = SimdBiquad::new(coeffs);
        f.process_8_channels(in_ptrs, out_ptrs, N);
        outs
    };

    let out_base = run(dispatch::LEVEL_BASELINE);
    let out_fast = run(dispatch::LEVEL_AVX2_FMA);
    dispatch::force_level(dispatch::LEVEL_AVX2_FMA);

    for c in 0..8 {
        let d = max_rel_diff(&out_fast[c], &out_base[c]);
        assert!(
            d < 1e-3,
            "channel {c}: the avx2 instantiation diverges from the baseline one by {d:.3e} \
             relative. FMA's single rounding vs multiply-then-add's two accounts for ~1e-6 \
             fed back through a resonant biquad; {d:.3e} is a real difference in the kernel."
        );
        assert!(
            out_fast[c].iter().all(|v| v.is_finite()),
            "channel {c}: avx2 path produced a non-finite sample"
        );
        assert!(
            out_base[c].iter().all(|v| v.is_finite()),
            "channel {c}: baseline path produced a non-finite sample"
        );
        // And neither is silently doing nothing — a kernel that output its
        // input unchanged would pass every comparison above.
        let moved = out_fast[c]
            .iter()
            .zip(&ins[c])
            .any(|(&o, &i)| (o - i).abs() > 1e-4);
        assert!(moved, "channel {c}: output equals input — the filter did not run");
    }
}

/// The baseline instantiation must be exercised even on a modern CPU.
///
/// Without `force_level` every developer machine with AVX2 would test only the
/// fast path, and the portable one — the one that actually ships to older
/// hardware — would go out untested.
#[cfg(target_arch = "x86_64")]
#[test]
fn baseline_instantiation_is_exercised() {
    let _guard = serialize();
    dispatch::prewarm();
    let forced = dispatch::force_level(dispatch::LEVEL_BASELINE);
    assert_eq!(
        forced,
        dispatch::LEVEL_BASELINE,
        "force_level must be able to step DOWN to the baseline on any CPU"
    );
    assert!(!dispatch::has_avx2_fma(), "forcing the baseline must change what dispatch selects");

    const N: usize = 1024;
    let ins: Vec<Vec<f32>> = (0..8).map(|c| signal(N, 0x3000 + c as u32)).collect();
    let in_ptrs: [*const f32; 8] = std::array::from_fn(|i| ins[i].as_ptr());
    let mut outs: Vec<Vec<f32>> = (0..8).map(|_| vec![0.0f32; N]).collect();
    let out_ptrs: [*mut f32; 8] = std::array::from_fn(|i| outs[i].as_mut_ptr());

    let mut f = SimdBiquad::new(BiquadCoefficients::linkwitz_riley_lp(1200.0, 44_100.0));
    f.process_8_channels(in_ptrs, out_ptrs, N);
    assert!(outs.iter().all(|o| o.iter().all(|v| v.is_finite())));

    dispatch::force_level(dispatch::LEVEL_AVX2_FMA);
}

/// Raising the level past what the CPU has must be refused, not obeyed —
/// obeying it would execute an illegal instruction.
#[test]
fn force_level_cannot_exceed_the_cpu() {
    let _guard = serialize();
    dispatch::prewarm();
    let real = dispatch::level();
    let got = dispatch::force_level(255);
    assert!(got <= real, "force_level raised past the CPU's real level ({got} > {real})");
    dispatch::force_level(real);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn summing_paths_agree() {
    dispatch::prewarm();
    if !dispatch::has_avx2_fma() {
        eprintln!("skipped: no avx2+fma on this CPU");
        return;
    }

    const N: usize = 4096;
    let ins: Vec<Vec<f32>> = (0..12).map(|c| signal(N, 0x2000 + c as u32)).collect();
    let refs: Vec<&[f32]> = ins.iter().map(|v| v.as_slice()).collect();

    let node = SummingNode::new();

    let mut simd = vec![0.0f32; N];
    node.process_16_to_1_simd(&refs, &mut simd);

    // The plain scalar summing path in the same type.
    let mut scalar = vec![0.0f32; N];
    node.process_16_to_1(&refs, &mut scalar);

    let d = max_rel_diff(&simd, &scalar);
    assert!(
        d < 1e-5,
        "dispatched bus sum diverges from the scalar sum by {d:.3e} relative. A sum of 12 \
         inputs reorders additions between the two, which costs a few ULP, not {d:.3e}."
    );
}
