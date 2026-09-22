//! Resampler cost: the vectorised unit-stretch path vs the general scalar walk.
//!
//! Both live in this one binary, so the comparison is interleaved inside a
//! single process. `stretch == 1.0` selects the fast path; `1.0 + EPSILON` is
//! numerically the same kernel but takes the general path, which is what the
//! fast path replaced — so this is a true before/after, not a proxy.
use audio_dsp::{dispatch, measurement::tone_sample, resample};
use std::time::{Duration, Instant};

const SR: f32 = 48_000.0;
const N: usize = 200_000;
const ROUNDS: usize = 5;

fn median(v: &[Duration]) -> Duration { let mut v = v.to_vec(); v.sort_unstable(); v[v.len()/2] }

fn run(src: &[f32], stretch: f32, rate: f64) -> Duration {
    let t = resample::table();
    let start = Instant::now();
    let mut acc = 0.0f32;
    for i in 0..N {
        acc += t.sample(src, 512.0 + i as f64 * rate, stretch);
    }
    let e = start.elapsed();
    std::hint::black_box(acc);
    e
}

fn ns(d: Duration) -> f64 { d.as_secs_f64() * 1e9 / N as f64 }

fn main() {
    dispatch::prewarm();
    println!("dispatch: {}   ({ROUNDS} rounds x {N} samples, median)\n", dispatch::level_name());

    let src: Vec<f32> = (0..(N as f64 * 1.1) as usize + 4096)
        .map(|i| tone_sample(i, 997.0, SR, 0.5)).collect();

    // Warm.
    run(&src, 1.0, 0.9172);
    run(&src, 1.0 + f32::EPSILON, 0.9172);

    let (mut fast, mut slow) = (vec![], vec![]);
    for _ in 0..ROUNDS {
        // rate <= 1.0 -> stretch clamps to 1.0 -> fast path.
        fast.push(run(&src, 1.0, 0.9172));
        // Same work, forced down the general path — this is what the fast path
        // replaced, so it is a true before/after rather than a proxy.
        slow.push(run(&src, 1.0 + f32::EPSILON, 0.9172));
    }
    let (m_slow, m_fast) = (ns(median(&slow)), ns(median(&fast)));
    println!("stretch 1.0 — every rate at or below 1.0 clamps here,");
    println!("so this is all tempo-down and all pitch-down:");
    println!("  general scalar walk (was)   {m_slow:>7.1} ns/sample");
    println!("  polyphase dot product       {m_fast:>7.1} ns/sample   {:.2}x", m_slow/m_fast);
    println!();
    println!("  NOT runtime-dispatched: an avx2+fma instantiation measured");
    println!("  reproducibly SLOWER (27.1 vs 25.4 ns). 16 elements is too few to");
    println!("  win throughput, and the horizontal reduction costs more in one");
    println!("  256-bit register than in two 128-bit ones.");

    // Stretched: unchanged, still the general path. Reported so a regression
    // there cannot hide behind the fast path's win.
    let mut st = vec![];
    for _ in 0..ROUNDS { st.push(run(&src, 1.0293, 1.0293)); }
    println!("\nstretch 1.0293 (+2.5% tempo UP — general path, unchanged):");
    println!("  general scalar walk         {:>7.1} ns/sample", ns(median(&st)));
}
