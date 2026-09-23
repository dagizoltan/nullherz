//! **Where does the console's THD+N and its latency come from, stage by stage?**
//!
//! `probe_signal_quality` measures the whole chain at once and reports about
//! -106 dB at every level. `probe_deck_latency` measures the whole chain at once
//! and reports 7.4 ms. Neither says WHICH stage is responsible, and after the
//! resampler reached -132 dB the chain became the limit — so the question is now
//! which of a dozen processors sets it.
//!
//! # How this taps mid-chain without touching graph internals
//!
//! `buffer_pool` is `pub(crate)`, so an example cannot read intermediate
//! buffers. It does not need to: `TopologyCommand::SetBypass` makes a node a
//! PASSTHROUGH (`run_job` copies `inputs[0]` to every output), so bypassing a
//! suffix of the chain and measuring at master gives the cumulative figure for
//! the prefix. Walking the prefix from one stage to the whole strip turns the
//! master output into a movable tap point.
//!
//! Two consequences worth knowing:
//!
//!   * A bypassed node copies input 0 to ALL outputs, so the right channel
//!     becomes a copy of the left. Everything here analyses the LEFT channel, so
//!     that is harmless — but it is why this is not a stereo measurement.
//!   * A bypassed SUMMING node passes only its first input. Deck A is input 0 of
//!     every sum on its path, which is exactly what a deck-A-only test wants.
//!
//! # Reading the result
//!
//! The cumulative column is what a listener would get if the chain ended there.
//! The delta column is what the stage itself added, and is the actionable one —
//! though note that THD+N deltas do not add linearly in dB, so a stage that
//! moves the total from -130 to -107 is responsible for essentially all of it.
//!
//!   cargo run --release -p nullherz-conductor --example probe_chain_taps

use std::sync::Arc;

use audio_dsp::measurement::{thd_n, tone_sample};
use nullherz_conductor::Conductor;
use nullherz_dna::GeneticLibrary;
use nullherz_traits::{Command, PerformanceCommand, TopologyCommand};

const SR: f32 = 48_000.0;
const BLOCK: usize = 256;
const FFT: usize = 32_768;
const TONE_HZ: f32 = 997.0;
const AMP: f32 = 0.5;
/// Long enough to outlast install, arm, warmup and the analysis window.
const TONE_SECONDS: f32 = 30.0;
const INSTALL_BLOCKS: usize = 512;
const WARMUP_BLOCKS: usize = 64;

fn db(x: f32) -> f32 { 20.0 * x.max(1e-20).log10() }
fn ms(n: usize) -> f32 { n as f32 / SR * 1000.0 }

/// A sustained tone, and a second source that is silent then steps to full
/// scale — a single-sample impulse smears below the detection threshold through
/// an analysis window, which is what made the first version of
/// `probe_deck_latency` report "no output at all".
fn register_sources(c: &Conductor) {
    let frames = (SR * TONE_SECONDS) as usize;

    let tone: Vec<f32> = (0..frames * 2)
        .map(|i| tone_sample(i % frames, TONE_HZ, SR, AMP))
        .collect();
    let mut onset = vec![0.0f32; frames * 2];
    // Silence for one block, then full scale, on both planes.
    for plane in 0..2 {
        for i in BLOCK..frames {
            onset[plane * frames + i] = if (i / 64) % 2 == 0 { 0.9 } else { -0.9 };
        }
    }

    for (id, buf) in [(7_001u64, tone), (7_002, onset)] {
        let mut md = nullherz_traits::SampleMetadata::new_empty();
        md.bpm = 120.0;
        md.total_samples = frames as u64;
        md.channels = 2;
        // MUST be set. `new_empty()` defaults to LEGACY_SOURCE_SAMPLE_RATE, and
        // a mismatch against the engine rate makes the deck RESAMPLE — which
        // moves a 997 Hz tone off the bin the analyser looks in and reports an
        // empty fundamental as -400 dB. The first version of this probe did
        // exactly that.
        md.sample_rate = SR as u32;
        let md = Arc::new(md);
        c.transfusion_manager
            .sample_registry
            .register_with_metadata(id, Arc::new(buf), md.clone());
        let lib = c.library.lock();
        lib.save_track(&nullherz_dna::LibraryTrack {
            id,
            path: format!("tone://{id}"),
            title: "probe".into(),
            artist: "probe".into(),
            album: "probe".into(),
            genre: "probe".into(),
            energy_level: 0.5,
            metadata: md,
        })
        .expect("in-memory save");
    }
}

fn pump(c: &mut Conductor, l: &mut [f32], r: &mut [f32]) {
    let n = l.len().min(r.len());
    let inputs: Vec<&[f32]> = vec![];
    let mut outputs = vec![l, r];
    let mut lock = c.engine_coordinator.backend_manager.engine_handle.lock();
    let e = lock.as_mut().expect("engine");
    if let Some(engine) = Arc::get_mut(e) {
        engine.process_block(&inputs, &mut outputs, n);
    } else {
        // Single-threaded harness; same pattern as the other probes.
        let p = Arc::as_ptr(e) as *mut dyn nullherz_traits::RenderingEngine;
        unsafe { (*p).process_block(&inputs, &mut outputs, n); }
    }
}

/// Bypass every node except `active`. Bypass is passthrough, so this leaves the
/// signal path intact and only removes processing.
fn set_active(c: &mut Conductor, node_count: u32, active: &[u32]) {
    let cmds: Vec<Command> = (0..node_count)
        .map(|n| {
            Command::Topology(TopologyCommand::SetBypass {
                node_idx: n,
                enabled: !active.contains(&n),
            })
        })
        .collect();
    c.apply_mixer_commands(cmds);
}

/// Load a source onto deck A and start it, with the deck settled first.
///
/// Load and Play MUST NOT be batched together. `LoadTrackToDeck` installs the
/// source through the TOPOLOGY ring while `PlayDeck` rides the COMMAND bus, and
/// the bus is drained against whatever graph is currently installed — so a Play
/// issued in the same batch arrives before the source exists and is dropped.
/// The deck then renders silence while every other signal in the probe looks
/// healthy, which is what made the first version of this sweep report -400 dB
/// from the second row onward.
fn play_source(c: &mut Conductor, sample_id: u64, l: &mut [f32], r: &mut [f32]) {
    c.apply_mixer_commands(vec![Command::Performance(PerformanceCommand::StopDeck { deck_id: 'A' })]);
    for _ in 0..8 { pump(c, l, r); }
    c.apply_mixer_commands(vec![Command::Performance(
        PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id })]);
    for _ in 0..16 { pump(c, l, r); }
    c.apply_mixer_commands(vec![Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' })]);
}

struct Tap {
    label: &'static str,
    /// Nodes switched ON at this tap, in addition to everything before it.
    nodes: Vec<u32>,
}

fn main() {
    let mut c = Conductor::with_library_path(":memory:");
    c.setup_engine();
    c.bootstrap_4channel_mixer();
    register_sources(&c);

    let mut l = vec![0.0f32; BLOCK];
    let mut r = vec![0.0f32; BLOCK];
    for _ in 0..INSTALL_BLOCKS { pump(&mut c, &mut l, &mut r); }

    let deck = c.mixer_manager.deck_mappings[&'A'].clone();
    let name = |k: &str| c.mixer_manager.node_names.get(k).copied();

    // Signal order down deck A and out through the master chain. Nodes that
    // exist but are not named (the per-bus summing pair) stay bypassed until the
    // final row, where they are passthrough anyway for a single-deck test.
    let mut taps: Vec<Tap> = vec![
        Tap { label: "sampler (source)",      nodes: vec![deck.sampler_id] },
        Tap { label: "+ pitch slot",          nodes: vec![deck.pitch_slot_id] },
        Tap { label: "+ dna slot",            nodes: vec![deck.dna_slot_id] },
        Tap { label: "+ deck gain",           nodes: vec![deck.gain_id] },
        Tap { label: "+ deck filter",         nodes: vec![deck.filter_id] },
        Tap { label: "+ stereo util",         nodes: vec![deck.stereo_util_id] },
    ];
    for (i, fx) in deck.fx_slot_ids.iter().enumerate() {
        taps.push(Tap { label: if i == 0 { "+ fx slot 1" } else { "+ fx slot n" }, nodes: vec![*fx] });
    }
    taps.push(Tap { label: "+ deck isolator/EQ", nodes: vec![deck.isolator_id] });
    if let (Some(a), Some(b)) = (name("master_xf_l"), name("master_xf_r")) {
        taps.push(Tap { label: "+ crossfader", nodes: vec![a, b] });
    }
    if let (Some(a), Some(b)) = (name("master_sum_l"), name("master_sum_r")) {
        taps.push(Tap { label: "+ master sum", nodes: vec![a, b] });
    }
    if let Some(n) = name("master_eq") {
        taps.push(Tap { label: "+ mastering EQ", nodes: vec![n] });
    }
    if let Some(n) = name("master_limiter") {
        taps.push(Tap { label: "+ master limiter", nodes: vec![n] });
    }

    let node_count = c.mixer_manager.id_allocator.current_node_id();

    println!("Console tap sweep — deck A only, left channel, {TONE_HZ} Hz at {:.1} dBFS.", db(AMP));
    println!("Bypass is PASSTHROUGH, so each row is the chain up to and including that stage.");
    println!("Analyser floor is about -135 dB; nothing below that is resolvable.\n");
    println!("  {:<24} {:>11} {:>9}   {:>10} {:>9}   {}", "tap point", "THD+N", "delta", "latency", "delta", "level");

    let mut active: Vec<u32> = Vec::new();
    let mut prev_thd: Option<f32> = None;
    let mut prev_lat: Option<usize> = None;

    for tap in &taps {
        active.extend_from_slice(&tap.nodes);
        set_active(&mut c, node_count, &active);
        for _ in 0..8 { pump(&mut c, &mut l, &mut r); }

        // ---- THD+N on the sustained tone ----
        play_source(&mut c, 7_001, &mut l, &mut r);
        for _ in 0..WARMUP_BLOCKS { pump(&mut c, &mut l, &mut r); }
        let mut captured: Vec<f32> = Vec::with_capacity(FFT + BLOCK);
        while captured.len() < FFT {
            pump(&mut c, &mut l, &mut r);
            captured.extend_from_slice(&l);
        }
        let t = thd_n(&captured[..FFT], TONE_HZ, SR, FFT);
        let peak = captured[..FFT].iter().fold(0.0f32, |a, v| a.max(v.abs()));

        // ---- latency on the onset source, from a cold deck ----
        play_source(&mut c, 7_002, &mut l, &mut r);
        let mut first: Option<usize> = None;
        'outer: for blk in 0..64 {
            pump(&mut c, &mut l, &mut r);
            for (i, v) in l.iter().enumerate() {
                if v.abs() > 1e-4 {
                    first = Some(blk * BLOCK + i);
                    break 'outer;
                }
            }
        }
        c.apply_mixer_commands(vec![
            Command::Performance(PerformanceCommand::StopDeck { deck_id: 'A' }),
        ]);
        for _ in 0..8 { pump(&mut c, &mut l, &mut r); }

        let tdb = db(t);
        let dthd = prev_thd.map(|p| format!("{:+.1}", tdb - p)).unwrap_or_else(|| "—".into());
        let (lat_s, dlat) = match first {
            Some(n) => (
                format!("{n} sp / {:.2} ms", ms(n)),
                prev_lat.map(|p| format!("{:+}", n as isize - p as isize)).unwrap_or_else(|| "—".into()),
            ),
            None => ("never arrived".into(), "—".into()),
        };
        // A dead row must look dead. Without this, silence reads as a
        // spectacular THD+N figure rather than as a broken measurement.
        let flag = if peak < 1e-6 { "  <- SILENT" } else { "" };
        println!("  {:<24} {:>8.1} dB {:>9}   {:>10} {:>9}   peak {:.4}{}",
            tap.label, tdb, dthd, lat_s, dlat, peak, flag);
        prev_thd = Some(tdb);
        if let Some(n) = first { prev_lat = Some(n); }
    }

    println!("\nDeltas in dB do not add: a stage that moves the total from -130 to -107");
    println!("is responsible for essentially all of the chain's distortion, and the");
    println!("stages after it only pass it on. Read the column for the step change.");
    println!("Latency deltas ARE additive, and a stage showing 0 declares nothing.");
}
