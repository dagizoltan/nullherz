//! Is a declared `latency_samples()` the delay the processor actually has?
//!
//! `pdc_latency_test` already pins the PLUMBING: the registry reports intrinsic
//! latency per type, the conductor's compiled plan carries it, engaging KeySync
//! adds its declared value. None of that checks the declaration is TRUE.
//!
//! It matters because PDC compensates by the declared number. If a processor
//! declares `fft.size` but really delays `fft.size - hop`, every path through it
//! is over-compensated by a hop — 256 samples is 5.3 ms at 48 kHz, which
//! presents as two decks drifting out of alignment when one carries an FFT
//! insert and the other does not.
//!
//! Measured here as the delay of the output's energy centroid against an
//! impulse, which survives the smearing an overlap-add pipeline applies (a
//! peak-position measure does not).
//!
//! Run: cargo run --release -p nullherz-processors --example probe_declared_latency

use nullherz_processors::registry::ProcessorRegistry;
use nullherz_traits::{ProcessContext, Transport};

const BLOCK: usize = 256;
const SAMPLE_RATE: f32 = 48_000.0;
/// Enough blocks to contain any FFT pipeline's latency plus its tail.
const BLOCKS: usize = 64;

fn transport() -> Transport {
    Transport {
        bpm: 120.0,
        beat_position: 0.0,
        is_playing: true,
        sample_rate: SAMPLE_RATE,
        absolute_samples: 0,
        system_time_ns: 0,
        device_time_ns: 0,
    }
}

/// Feed one impulse and return the full stereo-summed output tail.
fn impulse_tail(proc: &mut Box<dyn nullherz_traits::AudioProcessor>) -> Vec<f32> {
    let t = transport();
    let mut out_all = Vec::with_capacity(BLOCK * BLOCKS);
    for b in 0..BLOCKS {
        let mut in_l = [0.0f32; BLOCK];
        let mut in_r = [0.0f32; BLOCK];
        if b == 0 {
            in_l[0] = 1.0;
            in_r[0] = 1.0;
        }
        let mut out_l = [0.0f32; BLOCK];
        let mut out_r = [0.0f32; BLOCK];
        {
            let ins: [&[f32]; 2] = [&in_l, &in_r];
            let (a, c) = (&mut out_l[..], &mut out_r[..]);
            let mut outs: [&mut [f32]; 2] = [a, c];
            let mut ctx = ProcessContext {
                transport: Some(&t),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };
            proc.process(&ins, &mut outs, &mut ctx);
        }
        for i in 0..BLOCK {
            out_all.push(out_l[i].abs().max(out_r[i].abs()));
        }
    }
    out_all
}

/// Energy-weighted centroid of the tail, in samples. `None` when the processor
/// emitted nothing measurable.
fn centroid(tail: &[f32]) -> Option<f32> {
    let total: f64 = tail.iter().map(|v| (*v as f64) * (*v as f64)).sum();
    if total <= 1e-12 {
        return None;
    }
    let weighted: f64 = tail
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64) * (*v as f64) * (*v as f64))
        .sum();
    Some((weighted / total) as f32)
}

/// First sample at which the output rises meaningfully — the onset, which for a
/// pure delay IS the latency.
fn onset(tail: &[f32]) -> Option<usize> {
    let peak = tail.iter().fold(0.0f32, |m, v| m.max(*v));
    if peak <= 1e-6 {
        return None;
    }
    tail.iter().position(|v| *v > peak * 0.05)
}

fn main() {
    let registry = ProcessorRegistry::new();
    println!("declared latency vs measured impulse response\n");
    println!(
        "{:<26} {:>9}  {:>9}  {:>10}  {}",
        "processor", "declared", "onset", "centroid", "reading"
    );
    println!("{}", "-".repeat(82));

    for (id, type_name) in registry.list_available_processors() {
        let Some(mut proc) = registry.create_by_id(id, 0, SAMPLE_RATE) else { continue };
        let declared = proc.latency_samples();
        // Only the processors that claim a latency are interesting here; the
        // rest were shown to have none by inspection (no delay state at all).
        if declared == 0 {
            continue;
        }
        let tail = impulse_tail(&mut proc);
        match (onset(&tail), centroid(&tail)) {
            (Some(on), Some(cen)) => {
                let note = if (on as i64 - declared as i64).abs() <= BLOCK as i64 {
                    "onset within one block of declared"
                } else {
                    "ONSET DISAGREES with declared"
                };
                println!("{type_name:<26} {declared:>9}  {on:>9}  {cen:>10.0}  {note}");
            }
            _ => println!(
                "{type_name:<26} {declared:>9}  {:>9}  {:>10}  emitted nothing at default params",
                "-", "-"
            ),
        }
    }

    println!("\nReading it:");
    println!("  * 'declared' is latency_samples(), which is what PDC compensates by.");
    println!("  * 'onset' is the first output above 5% of peak. For a PURE DELAY — a");
    println!("    lookahead limiter — that is exactly the latency, and the Limiter row");
    println!("    is therefore a real verification.");
    println!();
    println!("  * THIS PROBE CANNOT JUDGE AN FFT PIPELINE, and its numbers should not be");
    println!("    read as if it could. An overlap-add pipeline synthesises its first");
    println!("    output through the rising half of a window, so a 5%-of-peak threshold");
    println!("    lands well past the algorithmic latency: PersonalityInheritance");
    println!("    measures an onset of ~1187 against a declared 1024, and the 163-sample");
    println!("    gap is the window fade-in, not an error in the declaration. With");
    println!("    hop_size = fft.size / 2 the algorithmic latency IS the window length,");
    println!("    so `fft.size` is the correct and conventional declaration.");
    println!();
    println!("  * 'emitted nothing' is not a pass either: Spectral, KeySync and");
    println!("    SpectralMorph are bypassed at default parameters, so there is no");
    println!("    response to measure. Verifying those needs each one driven into an");
    println!("    active state first, which is per-processor work this probe does not do.");
}
