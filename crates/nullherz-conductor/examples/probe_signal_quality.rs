//! **Is the console TRANSPARENT at unity?**
//!
//! Every other probe in this directory measures speed. This one measures
//! fidelity, which is the other half of "high-performance studio software" and
//! the half nothing here had ever checked.
//!
//! The question a studio tool has to answer is: if I set everything to unity and
//! ask for no processing, do I get my signal back? Anything else is colour the
//! operator did not request. Measures, on the real bootstrapped console:
//!
//!   * **THD+N** — a pure tone in, everything that is not that tone out.
//!   * **Level accuracy** — does unity gain mean unity.
//!   * **Idle noise floor** — what the console emits with no source at all.
//!
//!   cargo run --release -p nullherz-conductor --example probe_signal_quality

use std::sync::Arc;
use nullherz_traits::{Command, PerformanceCommand};

const SR: f32 = 48_000.0;
const BLOCK: usize = 256;
const FFT: usize = 16_384;

fn pump(c: &mut nullherz_conductor::Conductor, l: &mut [f32], r: &mut [f32]) {
    let n = l.len();
    let inputs: Vec<&[f32]> = vec![];
    let mut outs = vec![l, r];
    let mut lock = c.engine_coordinator.backend_manager.engine_handle.lock();
    let arc = lock.as_mut().expect("engine");
    let p = Arc::as_ptr(arc) as *mut dyn nullherz_traits::RenderingEngine;
    unsafe { (*p).process_block(&inputs, &mut outs, n); }
}

fn db(x: f32) -> f32 { 20.0 * x.max(1e-20).log10() }

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() { return 0.0; }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// THD+N against the bin holding `freq`, using a Hann window so leakage does not
/// masquerade as distortion.
fn thd_n(x: &[f32], freq: f32) -> (f32, f32) {
    let mut re = vec![0.0f32; FFT];
    let mut im = vec![0.0f32; FFT];
    for i in 0..FFT.min(x.len()) {
        let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FFT as f32).cos();
        re[i] = x[i] * w;
    }
    let fft = audio_dsp::SimdFft::new(FFT);
    fft.process(&mut re, &mut im);

    let mags: Vec<f32> = (0..FFT / 2).map(|b| (re[b] * re[b] + im[b] * im[b]).sqrt()).collect();
    let fund_bin = (freq / SR * FFT as f32).round() as usize;

    // A windowed tone occupies a few bins; +/-3 was NOT enough — it left enough
    // main-lobe energy outside the fundamental to register a 0.63% floor, which
    // is above where a clean digital path actually sits. Widened until the
    // control signal measures clean.
    let mut fund_energy = 0.0f64;
    let mut total_energy = 0.0f64;
    for (b, m) in mags.iter().enumerate() {
        let e = (*m as f64) * (*m as f64);
        total_energy += e;
        if b.abs_diff(fund_bin) <= 24 { fund_energy += e; }
    }
    let rest = (total_energy - fund_energy).max(0.0);
    let thd = if fund_energy > 0.0 { (rest / fund_energy).sqrt() as f32 } else { 0.0 };
    (thd, mags[fund_bin])
}

fn main() {
    let mut c = nullherz_conductor::Conductor::with_library_path(":memory:");
    c.setup_engine();
    c.bootstrap_4channel_mixer();

    let mut l = vec![0.0f32; BLOCK];
    let mut r = vec![0.0f32; BLOCK];
    for _ in 0..256 { pump(&mut c, &mut l, &mut r); }

    // ---- idle noise floor -------------------------------------------------
    let mut idle = Vec::with_capacity(FFT);
    while idle.len() < FFT {
        l.fill(0.0); r.fill(0.0);
        pump(&mut c, &mut l, &mut r);
        idle.extend_from_slice(&l);
    }
    // CONTROL: measure the analyser against a signal that has NOT been through
    // the console. Any THD it reports here is the measurement's own floor, and
    // every number below has to be read against it. Without this control the
    // first run of this probe would have reported the console's THD as 0.635%
    // when much of that is window leakage.
    {
        let pure: Vec<f32> = (0..FFT)
            .map(|i| (i as f32 * 997.0 * std::f32::consts::TAU / SR).sin() * 0.5)
            .collect();
        let (t, _) = thd_n(&pure, 997.0);
        println!("=== CONTROL: analyser floor on a pure sine (never touched the console) ===");
        println!("  THD+N of the measurement itself: {:.5}%  ({:.1} dB)", t * 100.0, db(t));
        println!("  Any console reading at or near this number is MEASUREMENT, not distortion.\n");
    }

    println!("=== idle (no source loaded) ===");
    println!("  noise floor: {:.1} dBFS rms, peak {:.1} dBFS", db(rms(&idle)), db(idle.iter().fold(0.0f32, |a, v| a.max(v.abs()))));

    // ---- pure tone through one deck at unity ------------------------------
    const TONE: f32 = 997.0; // not a bin-centre multiple; avoids flattering the FFT
    for amp_db in [-6.0f32, -12.0, -20.0, -40.0] {
        // FRESH console each time: reusing one across stop/load/play left the
        // deck silent and produced a bogus -400 dBFS row on the first attempt.
        let mut c = nullherz_conductor::Conductor::with_library_path(":memory:");
        c.setup_engine();
        c.bootstrap_4channel_mixer();
        let mut l = vec![0.0f32; BLOCK];
        let mut r = vec![0.0f32; BLOCK];
        for _ in 0..256 { pump(&mut c, &mut l, &mut r); }
        let amp = 10f32.powf(amp_db / 20.0);
        let frames = SR as usize * 4;
        let mut samples = vec![0.0f32; frames * 2];
        for ch in 0..2 {
            for i in 0..frames {
                samples[ch * frames + i] = (i as f32 * TONE * std::f32::consts::TAU / SR).sin() * amp;
            }
        }
        let mut meta = nullherz_traits::SampleMetadata::new_empty();
        meta.bpm = 120.0;
        meta.total_samples = frames as u64;
        meta.channels = 2;
        meta.sample_rate = SR as u32;
        let id = 900 + (-amp_db) as u64;
        c.transfusion_manager.sample_registry.register_with_metadata(id, Arc::new(samples), Arc::new(meta));

        c.apply_mixer_commands(vec![
            Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: id }),
            Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
        ]);

        // Let the chain settle, then capture steady state.
        for _ in 0..256 { l.fill(0.0); r.fill(0.0); pump(&mut c, &mut l, &mut r); }
        let mut out = Vec::with_capacity(FFT);
        while out.len() < FFT {
            l.fill(0.0); r.fill(0.0);
            pump(&mut c, &mut l, &mut r);
            out.extend_from_slice(&l);
        }

        // Localise any gain error: peak at each named node down the chain.
        if (amp_db - -6.0).abs() < 0.01 {
            let mut tel = nullherz_traits::telemetry::Telemetry::default();
            c.update_timeline(&mut tel);
            let peaks = tel.peak_levels;
            let mut named: Vec<(&String, &u32)> = c.mixer_manager.node_names.iter().collect();
            named.sort_by_key(|(_, v)| **v);
            println!("\n  --- per-node peak (input tone peak {:.4}) ---", amp);
            for (name, idx) in named {
                if name.starts_with("deck_a") || name.starts_with("master") {
                    let pk = peaks[*idx as usize];
                    println!("      {:<22} node {:>3}  peak {:.4}  ({:+.2} dB vs input)",
                        name, idx, pk, db(pk) - db(amp));
                }
            }
        }

        let (thd, _) = thd_n(&out, TONE);
        let out_rms = rms(&out);
        // A full-scale sine has rms = amp/sqrt(2).
        let expected_rms = amp / std::f32::consts::SQRT_2;
        println!("\n=== {TONE} Hz sine at {amp_db:.0} dBFS, deck A, everything at unity ===");
        println!("  output rms   : {:.1} dBFS (input {:.1} dBFS)", db(out_rms), db(expected_rms));
        println!("  gain error   : {:+.2} dB", db(out_rms) - db(expected_rms));
        println!("  THD+N        : {:.5}%  ({:.1} dB)", thd * 100.0, db(thd));

    }

    println!("\nReading it: a studio signal path at unity should be within a small");
    println!("fraction of a dB, and THD+N should sit near the float noise floor when");
    println!("nothing non-linear is engaged. Audible distortion thresholds are around");
    println!("0.1% for broadband material; converters and analogue desks are 0.001-0.01%.");
}
