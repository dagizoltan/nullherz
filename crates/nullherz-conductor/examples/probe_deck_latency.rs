//! **What is the actual end-to-end latency of a deck chain, and does the graph
//! know about it?**
//!
//! CPU headroom and latency are different questions. The console has ~7x headroom
//! at 256 samples (`bench_console_block`), but headroom says nothing about how
//! long a sample takes to travel from the deck to the output — and for a DJ
//! console that delay is the product.
//!
//! Measures both numbers and compares them:
//!   * REPORTED — what the graph believes, summed from `latency_samples()`, which
//!     is what plugin-delay compensation aligns decks with.
//!   * ACTUAL   — where an impulse fed into a deck actually emerges at master.
//!
//! A gap between them is worse than latency itself: PDC is *confidently* aligning
//! decks using a number that is wrong.
//!
//!   cargo run --release -p nullherz-conductor --example probe_deck_latency

use std::sync::Arc;
use nullherz_traits::{Command, PerformanceCommand};

const SR: f32 = 48_000.0;
fn block() -> usize {
    std::env::var("NULLHERZ_PROBE_BLOCK").ok().and_then(|v| v.parse().ok()).unwrap_or(256)
}

fn pump(c: &mut nullherz_conductor::Conductor, l: &mut [f32], r: &mut [f32]) {
    let l_len = l.len();
    let inputs: Vec<&[f32]> = vec![];
    let mut outs = vec![l, r];
    let mut lock = c.engine_coordinator.backend_manager.engine_handle.lock();
    let arc = lock.as_mut().expect("engine");
    let p = Arc::as_ptr(arc) as *mut dyn nullherz_traits::RenderingEngine;
    unsafe { (*p).process_block(&inputs, &mut outs, l_len); }
}

fn main() {
    let mut c = nullherz_conductor::Conductor::with_library_path(":memory:");
    c.setup_engine();
    c.bootstrap_4channel_mixer();

    // A full-scale tone from sample 0. A single-sample impulse is the cleaner
    // measurement in principle, but it does not survive a phase vocoder's
    // analysis window — the first attempt at this probe read "no output at all"
    // because the impulse was smeared below the detection threshold. A sustained
    // tone gives an unambiguous onset through the same window.
    let frames = 48_000;
    let mut samples = vec![0.0f32; frames * 2];
    for ch in 0..2 {
        for i in 0..frames {
            samples[ch * frames + i] = (i as f32 * 440.0 * std::f32::consts::TAU / SR).sin() * 0.9;
        }
    }

    let mut meta = nullherz_traits::SampleMetadata::new_empty();
    meta.bpm = 120.0;
    meta.total_samples = frames as u64;
    meta.channels = 2;
    meta.sample_rate = SR as u32;
    c.transfusion_manager
        .sample_registry
        .register_with_metadata(1, Arc::new(samples), Arc::new(meta));

    let bs = block();
    let mut l = vec![0.0f32; bs];
    let mut r = vec![0.0f32; bs];
    for _ in 0..512 { pump(&mut c, &mut l, &mut r); }

    // What does the GRAPH think the latency is?
    let reported = {
        let lock = c.engine_coordinator.backend_manager.engine_handle.lock();
        let arc = lock.as_ref().expect("engine");
        let p = Arc::as_ptr(arc) as *const dyn nullherz_traits::RenderingEngine;
        unsafe { (*p).list_children() }
            .iter()
            .filter_map(|ch| ch.metadata().map(|m| (m.processor_id as u32, ch.latency_samples())))
            .filter(|(_, lat)| *lat > 0)
            .collect::<Vec<_>>()
    };

    c.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: 1 }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
    ]);

    // Find the impulse at master.
    let mut first_out: Option<usize> = None;
    let mut peak = 0.0f32;
    'outer: for blk in 0..600 {
        l.fill(0.0);
        r.fill(0.0);
        pump(&mut c, &mut l, &mut r);
        for (i, v) in l.iter().enumerate() {
            peak = peak.max(v.abs());
            if v.abs() > 1e-4 && first_out.is_none() {
                first_out = Some(blk * bs + i);
                break 'outer;
            }
        }
    }

    let ms = |s: usize| s as f32 / SR * 1000.0;

    println!("=== what the graph BELIEVES (drives PDC) ===");
    if reported.is_empty() {
        println!("  no node in the bootstrapped console reports ANY latency.");
    } else {
        for (idx, lat) in &reported {
            println!("  node {idx:>3}: {lat} samples ({:.1} ms)", ms(*lat));
        }
    }

    println!("\n=== what actually happened ===");
    match first_out {
        Some(n) => println!("  impulse emerged at master after {n} samples ({:.1} ms), peak {peak:.4}", ms(n)),
        None => println!("  impulse never emerged (peak {peak:.6})"),
    }

    println!("\n=== the gap ===");
    let believed: usize = reported.iter().map(|(_, l)| *l).max().unwrap_or(0);
    match first_out {
        Some(n) => {
            println!("  believed (worst path): {believed} samples ({:.1} ms)", ms(believed));
            println!("  measured:              {n} samples ({:.1} ms)", ms(n));
            println!("  UNDECLARED:            {} samples ({:.1} ms)",
                n.saturating_sub(believed), ms(n.saturating_sub(believed)));
            println!("\n  A residual gap of a block or two is expected: the engine renders in");
            println!("  {bs}-sample blocks and this detects an onset through an analysis window,");
            println!("  so arrival quantises upward. A gap on the order of an FFT window is not");
            println!("  quantisation — it is a processor not declaring itself.");
        }
        None => println!("  no output to compare."),
    }
}
