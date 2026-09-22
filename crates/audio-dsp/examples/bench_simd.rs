//! A/B the runtime-dispatched DSP kernels against the compilation baseline.
//!
//! Both instantiations live in this one binary (see `audio_dsp::dispatch`), so
//! the comparison is INTERLEAVED inside a single process rather than taken
//! across two builds. That matters more than it sounds: on the reference box the
//! same binary varied 39% run to run, which is wider than the effect being
//! measured, so a build-to-build comparison cannot resolve it at all.
//!
//!   cargo run --release -p audio-dsp --example bench_simd

use audio_dsp::{dispatch, BiquadCoefficients, BiquadFilter, Filter, SimdBiquad, SummingNode};
use std::time::{Duration, Instant};

const BLOCK: usize = 128;
const ITERS: usize = 100_000;
/// Interleaved rounds. Alternating the two paths cancels slow drift (thermal,
/// background load) that a run-all-of-A-then-all-of-B ordering would attribute
/// to whichever path went second.
const ROUNDS: usize = 5;

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort_unstable();
    v[v.len() / 2]
}

fn bench_biquad_8ch(level: u8) -> Duration {
    dispatch::force_level(level);
    let coeffs = BiquadCoefficients::linkwitz_riley_lp(800.0, 44_100.0);
    let input = vec![0.25f32; BLOCK];
    let mut outs: Vec<Vec<f32>> = (0..8).map(|_| vec![0.0f32; BLOCK]).collect();
    let in_ptrs: [*const f32; 8] = [input.as_ptr(); 8];
    let out_ptrs: [*mut f32; 8] = std::array::from_fn(|i| outs[i].as_mut_ptr());
    let mut f = SimdBiquad::new(coeffs);

    let t = Instant::now();
    for _ in 0..ITERS {
        f.process_8_channels(in_ptrs, out_ptrs, BLOCK);
    }
    let e = t.elapsed();
    std::hint::black_box(&outs);
    e
}

fn bench_summing(level: u8) -> Duration {
    dispatch::force_level(level);
    let ins: Vec<Vec<f32>> = (0..12).map(|_| vec![0.3f32; BLOCK]).collect();
    let refs: Vec<&[f32]> = ins.iter().map(|v| v.as_slice()).collect();
    let mut out = vec![0.0f32; BLOCK];
    let node = SummingNode::new();

    let t = Instant::now();
    for _ in 0..ITERS {
        node.process_16_to_1_simd(&refs, &mut out);
    }
    let e = t.elapsed();
    std::hint::black_box(&out);
    e
}

fn bench_scalar_reference() -> Duration {
    let coeffs = BiquadCoefficients::linkwitz_riley_lp(800.0, 44_100.0);
    let input = vec![0.25f32; BLOCK];
    let mut filters: Vec<BiquadFilter> = (0..8).map(|_| BiquadFilter::new(coeffs)).collect();
    let mut outs: Vec<Vec<f32>> = (0..8).map(|_| vec![0.0f32; BLOCK]).collect();

    let t = Instant::now();
    for _ in 0..ITERS {
        for ch in 0..8 {
            for i in 0..BLOCK {
                outs[ch][i] = filters[ch].process_sample(input[i]);
            }
        }
    }
    let e = t.elapsed();
    std::hint::black_box(&outs);
    e
}

fn report(name: &str, base: Duration, fast: Duration, dispatch_available: bool) {
    let b = base.as_secs_f64() * 1e3;
    let f = fast.as_secs_f64() * 1e3;
    print!("  {name:<22} baseline {b:>8.1} ms   avx2+fma {f:>8.1} ms");
    if dispatch_available {
        println!("   {:.2}x", b / f);
    } else {
        println!("   (same code — no avx2+fma on this CPU)");
    }
}

fn main() {
    dispatch::prewarm();
    let has_fast = dispatch::level() == dispatch::LEVEL_AVX2_FMA;
    println!(
        "CPU dispatch level: {}   ({} rounds of {} iterations, {} frames, median)",
        dispatch::level_name(),
        ROUNDS,
        ITERS,
        BLOCK
    );
    if !has_fast {
        println!("  NOTE: no avx2+fma here, so both columns run identical code.");
    }
    println!();

    // Warm up: first touch pays for page faults and branch-predictor training.
    let _ = bench_biquad_8ch(dispatch::LEVEL_BASELINE);
    let _ = bench_summing(dispatch::LEVEL_BASELINE);

    let mut bq_base = Vec::new();
    let mut bq_fast = Vec::new();
    let mut sum_base = Vec::new();
    let mut sum_fast = Vec::new();

    for _ in 0..ROUNDS {
        bq_base.push(bench_biquad_8ch(dispatch::LEVEL_BASELINE));
        bq_fast.push(bench_biquad_8ch(dispatch::LEVEL_AVX2_FMA));
        sum_base.push(bench_summing(dispatch::LEVEL_BASELINE));
        sum_fast.push(bench_summing(dispatch::LEVEL_AVX2_FMA));
    }

    println!("Runtime-dispatched kernels:");
    report("biquad, 8 channels", median(bq_base), median(bq_fast), has_fast);
    report("bus sum, 12 inputs", median(sum_base), median(sum_fast), has_fast);

    println!();
    println!("For scale — the scalar reference, which cannot vectorise at all");
    println!("(a serial IIR recurrence has a loop-carried dependency):");
    println!(
        "  biquad, 8 channels     scalar   {:>8.1} ms",
        bench_scalar_reference().as_secs_f64() * 1e3
    );

    // Leave the process in its real state rather than whatever the last bench forced.
    dispatch::force_level(dispatch::LEVEL_AVX2_FMA);
}
