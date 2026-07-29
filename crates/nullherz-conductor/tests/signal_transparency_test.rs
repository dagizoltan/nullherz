//! **Is the console transparent at unity?**
//!
//! One deck, a pure tone, every control at unity, nothing engaged. The signal
//! chain is
//!
//! ```text
//! Sampler -> [pitch] -> [dna] -> Gain -> Biquad -> StereoUtility -> [fx]
//!         -> DjIsolator -> (bus sum) -> master sum -> MasteringEq -> Limiter
//! ```
//!
//! with every slot holding BYPASS. That is gain-and-sum arithmetic plus one
//! allpass-phase crossover re-sum: it must return the tone, not a coloured
//! version of it. This test pins that.
//!
//! It exists because the console was for a while measured at **0.046% THD+N
//! (-66.7 dB)** here, and two separate causes were found and fixed under that
//! one number:
//!
//! 1. A real one: `SummingProcessor` shipped `soft_clip: true`, an
//!    unconditional `tanh()` waveshaper on all seven summing nodes — 1.23% at
//!    -6 dBFS. This test is the guard that catches its return.
//! 2. A phantom one: the measurement's own tone generator accumulated phase in
//!    f32, so the stimulus itself carried 0.046% by the time the capture
//!    started. See `audio_dsp::measurement` for the mechanism; the guard for
//!    that lives beside it.
//!
//! The lesson worth keeping is that those two were indistinguishable from a
//! single THD number, and telling them apart needed a control taken through
//! the same window. This test carries that control with it.

use std::sync::Arc;

use audio_dsp::measurement::{analyser_floor, thd_n, tone_sample};
use nullherz_conductor::Conductor;
use nullherz_traits::{Command, PerformanceCommand};

const SR: f32 = 48_000.0;
const BLOCK: usize = 256;
const FFT: usize = 16_384;
/// Not a bin-centre multiple, so the FFT is not flattered.
const TONE_HZ: f32 = 997.0;

/// Blocks pumped before capture: topology install + command delivery + chain
/// settling. Fixed, not condition-based, so the window is identical every run.
const SETTLE_BLOCKS: usize = 256;

/// How far above the analyser's own floor a reading may sit before we call it
/// distortion.
///
/// The console measures 1.004x the floor — its own contribution is ~-107 dB,
/// below what this analyser resolves. 2x leaves generous room for float
/// reassociation while still failing hard on anything real: the `tanh()`
/// waveshaper that used to sit on the summing nodes lands ~200x the floor.
const FLOOR_MARGIN: f32 = 2.0;

fn pump(conductor: &mut Conductor, left: &mut [f32], right: &mut [f32]) {
    let inputs: Vec<&[f32]> = vec![];
    let mut outputs = vec![left, right];
    let mut engine_lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
    let engine_arc = engine_lock.as_mut().expect("setup_engine must install an engine");
    if let Some(engine) = Arc::get_mut(engine_arc) {
        engine.process_block(&inputs, &mut outputs, BLOCK);
    } else {
        // Shared with coordinator plumbing; no other thread runs in this test.
        let ptr = Arc::as_ptr(engine_arc) as *mut dyn nullherz_traits::RenderingEngine;
        unsafe { (*ptr).process_block(&inputs, &mut outputs, BLOCK); }
    }
}

/// Render `FFT` samples of the master L output with a `TONE_HZ` sine at `amp`
/// playing on deck A, everything else untouched.
fn render_master_at(amp: f32, sample_id: u64) -> Vec<f32> {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    let mut left = vec![0.0f32; BLOCK];
    let mut right = vec![0.0f32; BLOCK];
    // Install the bootstrap topology before anything is loaded onto it.
    for _ in 0..SETTLE_BLOCKS {
        pump(&mut conductor, &mut left, &mut right);
    }

    // Planar stereo: channel c occupies [c*frames, (c+1)*frames).
    let frames = SR as usize * 4;
    let mut samples = vec![0.0f32; frames * 2];
    for ch in 0..2 {
        for i in 0..frames {
            samples[ch * frames + i] = tone_sample(i, TONE_HZ, SR, amp);
        }
    }
    let mut meta = nullherz_traits::SampleMetadata::new_empty();
    meta.bpm = 120.0;
    meta.total_samples = frames as u64;
    meta.channels = 2;
    meta.sample_rate = SR as u32;
    conductor.transfusion_manager.sample_registry.register_with_metadata(
        sample_id,
        Arc::new(samples),
        Arc::new(meta),
    );

    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
    ]);

    // Settle: the limiter has look-ahead latency and the chain has filter
    // warm-up. Measuring the head of the output reads that ramp, not steady
    // state — an earlier probe "proved" a node did nothing by reading silence.
    for _ in 0..SETTLE_BLOCKS {
        left.fill(0.0);
        right.fill(0.0);
        pump(&mut conductor, &mut left, &mut right);
    }

    let mut out = Vec::with_capacity(FFT);
    while out.len() < FFT {
        left.fill(0.0);
        right.fill(0.0);
        pump(&mut conductor, &mut left, &mut right);
        out.extend_from_slice(&left);
    }
    out.truncate(FFT);
    out
}

/// The console at unity must add no distortion the analyser can see, at any
/// level.
///
/// Swept across 34 dB deliberately. A SATURATING non-linearity (the `tanh()`
/// waveshaper) grows with level and would fail loudest at -6 dBFS; a
/// STRUCTURAL one (a stuck ramp, a mis-derived filter, a precision path) is
/// level-independent and fails equally everywhere. Which rows fail is the
/// first bisection of any regression this catches.
#[test]
fn test_console_at_unity_adds_no_distortion() {
    let floor = analyser_floor(TONE_HZ, SR, FFT);
    let limit = floor * FLOOR_MARGIN;

    for (amp_db, id) in [(-6.0f32, 9006u64), (-20.0, 9020), (-40.0, 9040)] {
        let amp = 10f32.powf(amp_db / 20.0);
        let out = render_master_at(amp, id);

        // Guard the guard: a silent render measures 0% THD and would sail
        // through. The -6.09 dB the console currently loses at unity (master
        // crossfader at centre on a LINEAR law, plus -0.065 dB from the
        // isolator's band re-sum) is well inside this.
        let peak = out.iter().fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(
            peak > amp * 0.25,
            "deck A rendered near-silence at {amp_db} dBFS (peak {peak:.6}) — the \
             transparency measurement below would pass on silence, so this is a \
             broken fixture, not a clean console"
        );

        let measured = thd_n(&out, TONE_HZ, SR, FFT);
        assert!(
            measured <= limit,
            "console THD+N at {amp_db} dBFS is {:.5}%, above the {:.5}% limit \
             ({FLOOR_MARGIN}x the {:.5}% analyser floor). At unity with every \
             slot on BYPASS the chain is gain-and-sum plus an allpass re-sum; \
             it should be transparent. Suspect anything that shapes a waveform \
             unconditionally (a soft-clip default), a coefficient ramp that \
             never settles, or a filter that is not the identity it claims.",
            measured * 100.0, limit * 100.0, floor * 100.0
        );
    }
}

/// The residue must not be level-dependent.
///
/// This is the discriminator that a single THD reading cannot give you: a
/// saturating non-linearity distorts more as it is driven harder, so its THD
/// RISES with level. Anything structural stays flat. Catching the shape
/// separately from the magnitude is what separated the `tanh()` waveshaper
/// (1.23% at -6 dBFS, 0.046% at -40) from the phantom that survived it.
#[test]
fn test_console_residue_does_not_grow_with_level() {
    let hot = thd_n(&render_master_at(0.5, 9106), TONE_HZ, SR, FFT);
    let quiet = thd_n(&render_master_at(0.01, 9140), TONE_HZ, SR, FFT);

    assert!(
        hot <= quiet * 4.0,
        "THD+N rises with level ({:.5}% at -6 dBFS vs {:.5}% at -40 dBFS) — the \
         console has a SATURATING non-linearity in the path, not a structural \
         one. A waveshaper does not wait for a threshold; check for a soft-clip \
         or drive default that is on when nothing asked for it.",
        hot * 100.0, quiet * 100.0
    );
}
