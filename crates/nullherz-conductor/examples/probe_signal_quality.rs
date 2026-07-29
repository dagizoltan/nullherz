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
/// Blocks pumped before capture, and the sample offset into the source file
/// that puts the capture at. The CONTROL is taken at this same offset.
const SETTLE_BLOCKS: usize = 256;
const SETTLE_SAMPLES: usize = SETTLE_BLOCKS * BLOCK;

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

/// The tone generator and the analyser both live in `audio_dsp::measurement`,
/// not here — they are the measurement CONTRACT, and this probe is only one of
/// its callers (`signal_transparency_test` is the other, and the two have to be
/// grading on the same ruler to be comparable). That module's docs carry the
/// f32-phase trap this probe walked into, and its tests pin the analyser floor.
fn tone_sample(i: usize, freq: f32, amp: f32) -> f32 {
    audio_dsp::measurement::tone_sample(i, freq, SR, amp)
}

fn thd_n(x: &[f32], freq: f32) -> f32 {
    audio_dsp::measurement::thd_n(x, freq, SR, FFT)
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
    //
    // It must be the SAME WINDOW OF THE SAME BUFFER the console is graded on.
    // A control taken from sample 0 of a freshly generated sine grades the
    // analyser but not the STIMULUS, and the stimulus is where the last bogus
    // number lived: the deck is 65_536 samples into the file by the time the
    // capture starts, and a generator whose phase error grows with sample index
    // is a different, dirtier signal there than at sample 0. Reading a
    // sample-0 control against a sample-65_536 measurement compared two
    // different tones and charged the difference to the console.
    {
        let pure: Vec<f32> = (0..SETTLE_SAMPLES + FFT)
            .map(|i| tone_sample(i, 997.0, 0.5))
            .collect();
        let head = thd_n(&pure, 997.0);
        let at_capture = thd_n(&pure[SETTLE_SAMPLES..], 997.0);
        println!("=== CONTROL: analyser floor on a pure sine (never touched the console) ===");
        println!("  THD+N of the measurement itself: {:.5}%  ({:.1} dB)", at_capture * 100.0, db(at_capture));
        println!("  (same tone measured from sample 0: {:.5}% — these must agree, or the", head * 100.0);
        println!("   generator is drifting and the console is being blamed for it)");
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
                samples[ch * frames + i] = tone_sample(i, TONE, amp);
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
        for _ in 0..SETTLE_BLOCKS { l.fill(0.0); r.fill(0.0); pump(&mut c, &mut l, &mut r); }
        let mut out = Vec::with_capacity(FFT);
        while out.len() < FFT {
            l.fill(0.0); r.fill(0.0);
            pump(&mut c, &mut l, &mut r);
            out.extend_from_slice(&l);
        }

        // NOT a per-node peak table here. `Telemetry::peak_levels` is populated
        // by the RT-side finalizer on its own cadence, NOT by
        // `Conductor::update_timeline` — pulling it this way returns zeros, and
        // this block used to print a full chain of confident `-394.00 dB` rows
        // that read as "every node is silent". A bisect built on that is built
        // on nothing. To localise a stage, drive the kernels directly
        // (`probe_thd_bisect`) or render through progressively shorter chains.

        let thd = thd_n(&out, TONE);
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
