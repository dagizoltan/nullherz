// Non-RT plane (test-harness pacing): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]

//! Headless survival-test harness — the code half of the Validation Gate's
//! "Survival" test (docs/business/STRATEGIC_ASSESSMENT_2026_07.md §3).
//!
//! Boots the full 4-channel DJ topology on a real backend, loads the first two
//! analyzed tracks onto decks A/B, plays them, and consumes telemetry for the
//! requested duration while tracking xruns and DSP load. Writes a markdown
//! report and exits non-zero if any xrun occurred.
//!
//! Usage:
//!   cargo run --release -p nullherz-conductor --bin survival -- \
//!       [--minutes N] [--backend alsa|pipewire|jack|threaded|mock] \
//!       [--tracks DIR] [--report PATH]

use std::time::{Duration, Instant};

struct Args {
    minutes: u64,
    backend: Option<nullherz_traits::AudioBackendType>,
    tracks_dir: String,
    report_path: Option<String>,
}

fn parse_args() -> Args {
    let mut args = Args { minutes: 60, backend: None, tracks_dir: "tracks".to_string(), report_path: None };
    let argv: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--minutes" => {
                i += 1;
                args.minutes = argv.get(i).and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                    eprintln!("--minutes needs a number");
                    std::process::exit(2);
                });
            }
            "--backend" => {
                i += 1;
                args.backend = Some(match argv.get(i).map(|s| s.to_lowercase()).as_deref() {
                    Some("alsa") => nullherz_traits::AudioBackendType::Alsa,
                    Some("pipewire") => nullherz_traits::AudioBackendType::Pipewire,
                    Some("jack") => nullherz_traits::AudioBackendType::Jack,
                    Some("threaded") => nullherz_traits::AudioBackendType::Threaded,
                    Some("mock") => nullherz_traits::AudioBackendType::Mock,
                    other => {
                        eprintln!("unknown backend {:?}", other);
                        std::process::exit(2);
                    }
                });
            }
            "--tracks" => {
                i += 1;
                args.tracks_dir = argv.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("--tracks needs a directory");
                    std::process::exit(2);
                });
            }
            "--report" => {
                i += 1;
                args.report_path = argv.get(i).cloned();
            }
            "--help" | "-h" => {
                println!("survival [--minutes N] [--backend alsa|pipewire|jack|threaded|mock] [--tracks DIR] [--report PATH]");
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument {}", other);
                std::process::exit(2);
            }
        }
        i += 1;
    }
    args
}

/// Frames left untouched at the end of a loop.
///
/// The sampler voice needs four samples of lookahead for its interpolator and
/// deactivates when it gets within that of the buffer end — a check that runs
/// before the loop wrap. Eight gives that margin room to spare.
const LOOP_TAIL_GUARD: u64 = 8;

/// Settling time before underruns start counting.
///
/// The Gate 1 contract is "4 decks, WARM, 60 minutes, zero xruns" and the
/// harness never honoured the warm part. The first moments of a run are cold
/// caches, a CPU still ramping up from powersave, and threads that have not
/// been scheduled yet — a single transient there says nothing about whether the
/// console holds up, but it failed the whole run. They are reported separately
/// rather than hidden.
const WARMUP: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Stats {
    frames: u64,
    xrun_count_final: u32,
    xrun_events: Vec<(Duration, u32, u64)>, // (elapsed, cumulative count, magnitude_ns)
    peak_process_time_ns: u64,
    sum_process_time_ns: u64,
    resource_leaks_final: u64,
    sample_rate: f32,
    samples_processed: u64,
    /// Blocks whose process time exceeded the period budget: (elapsed, block ns).
    /// Timing tells load spikes (first seconds) apart from steady-state trouble.
    overrun_events: Vec<(Duration, u64)>,
    overrun_count: u64,
    /// Loudest sample the MASTER output produced across the run.
    ///
    /// Specifically the master limiter's output — the last node before the
    /// device — not the loudest node anywhere. Those are very different
    /// numbers: a summing bus routinely exceeds full scale, which is precisely
    /// what the limiter downstream of it exists to catch. Reporting the global
    /// max made the console look like it was clipping when the signal actually
    /// leaving it was fine.
    peak_master_level: f32,
    /// Loudest sample on any node, pre-limiter stages included. Not a clipping
    /// indicator — it is headroom telemetry, and >1.0 here is normal.
    ///
    /// Without this the harness happily reports PASS on a completely silent
    /// graph: zero xruns is trivially true when there is no audio to drop. The
    /// golden render and the block benchmark both already refuse to trust a
    /// silent run; this is the same guard for the one test that is supposed to
    /// prove the console survives real playback.
    peak_output_level: f32,
    /// Underruns as counted by the BACKEND, which is the only party that
    /// actually observes them. `None` = the running backend does not report.
    backend_xruns: Option<u64>,
    /// Underruns during warm-up, excluded from the verdict but reported.
    warmup_xruns: u64,
    /// Telemetry frames where the graph produced actual signal, so a run that
    /// starts loud and dies silent halfway is distinguishable from a good one.
    frames_with_signal: u64,
}

#[tokio::main]
async fn main() {
    let args = parse_args();

    println!("=== Nullherz Survival Harness ===");
    use nullherz_dna::GeneticLibrary as _;
    let mut conductor = nullherz_conductor::Conductor::new();
    let _ = conductor.load_system_config();
    let mut context = conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    if let Some(worker) = conductor.analysis_worker.take() {
        worker.start();
    }
    if let Some(monitor) = conductor.folder_monitor.take() {
        monitor.start_auto_scan(args.tracks_dir.clone());
    }

    // Resolve backend: CLI flag wins, then system_config.json, then ALSA.
    let backend = args.backend.unwrap_or_else(|| {
        std::fs::read_to_string("system_config.json")
            .ok()
            .and_then(|c| serde_json::from_str::<nullherz_conductor::persistence::SystemConfig>(&c).ok())
            .map(|cfg| match cfg.audio_backend.to_lowercase().as_str() {
                "pipewire" => nullherz_traits::AudioBackendType::Pipewire,
                "jack" => nullherz_traits::AudioBackendType::Jack,
                "threaded" => nullherz_traits::AudioBackendType::Threaded,
                "mock" => nullherz_traits::AudioBackendType::Mock,
                _ => nullherz_traits::AudioBackendType::Alsa,
            })
            .unwrap_or(nullherz_traits::AudioBackendType::Alsa)
    });

    println!("Backend: {:?}", backend);
    if let Err(e) = conductor.start_backend(backend) {
        eprintln!("FATAL: backend {:?} failed to start: {}", backend, e);
        eprintln!("(No automatic fallback here — a survival run on the wrong backend is meaningless.)");
        std::process::exit(2);
    }

    // Wait for the analysis pipeline to surface at least two tracks (up to 60s).
    println!("Waiting for track analysis in '{}'...", args.tracks_dir);
    let mut track_ids: Vec<u64> = Vec::new();
    let scan_deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < scan_deadline {
        {
            let lib = conductor.library.lock();
            if let Ok(tracks) = lib.list_tracks() {
                // Only tracks whose file actually exists: stale library entries
                // (e.g. old auto-breeder children) must not be selected.
                // LONGEST first, and only tracks whose file still exists
                // (stale library rows, e.g. old auto-breeder children, must not
                // be selected).
                //
                // Order matters. This used to take whatever `list_tracks()`
                // returned first, and the demo folder holds ten 5-12 second WAVs
                // alongside two ~140 s mixes — so the run's verdict depended on
                // which the library happened to list. Drawing short ones meant
                // the decks played out in the first ten seconds and the harness
                // failed on "audio stopped part-way" with a perfectly healthy
                // engine. Looping (below) covers the rest; this just makes the
                // choice deterministic and starts from the best material.
                let mut rows: Vec<_> = tracks.iter()
                    .filter(|t| std::path::Path::new(&t.path).exists())
                    .collect();
                rows.sort_by_key(|t| std::cmp::Reverse(t.metadata.total_samples));
                track_ids = rows.iter().map(|t| t.id).collect();
            }
        }
        conductor.tick();
        while let Some(mut tel) = context.telemetry_consumer.pop() {
            conductor.update_timeline(&mut tel);
        }
        if track_ids.len() >= 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    if track_ids.len() < 2 {
        eprintln!(
            "FATAL: needed 2 analyzed tracks in '{}', found {}. Put two WAVs there first.",
            args.tracks_dir,
            track_ids.len()
        );
        std::process::exit(2);
    }

    // Diagnostic: what does the ENGINE actually contain at this point?
    {
        let handle = conductor.engine_coordinator.backend_manager.engine_handle.lock();
        if let Some(engine) = handle.as_ref() {
            let children = engine.list_children();
            println!("DIAG: engine has {} child processors: {:?}",
                children.len(),
                children.iter().map(|c| c.processor_type()).collect::<Vec<_>>());
        } else {
            println!("DIAG: engine handle is EMPTY");
        }
        println!("DIAG: deck_mappings: {:?}",
            conductor.mixer_manager.deck_mappings.iter().map(|(k,v)| (*k, v.sampler_id)).collect::<Vec<_>>());
        println!("DIAG: registry ids: {:?}", conductor.transfusion_manager.sample_registry.list_ids());
    }
    println!("Loading track {} -> Deck A, track {} -> Deck B; starting playback.", track_ids[0], track_ids[1]);
    use nullherz_traits::{Command, PerformanceCommand};
    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: track_ids[0] }),
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'B', sample_id: track_ids[1] }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'B' }),
    ]);

    // Loop both decks for the whole run.
    //
    // The harness asserts that signal is present in >90% of frames, which is
    // the check that stops a silent graph reporting PASS. Without looping that
    // assertion is really a statement about track length: a 60-minute Gate 1
    // run against a 140-second mix is silent for 96% of it and fails no matter
    // how healthy the engine is. Looping makes the duration the operator asks
    // for the duration that actually gets tested.
    for (deck, id) in [('A', track_ids[0]), ('B', track_ids[1])] {
        let node = conductor.mixer_manager.deck_mappings.get(&deck).map(|n| n.sampler_id);
        let len = {
            let lib = conductor.library.lock();
            lib.get_track(id).ok().flatten().map(|t| t.metadata.total_samples).unwrap_or(0)
        };
        match (node, len) {
            (Some(node_idx), n) if n > LOOP_TAIL_GUARD => {
                // Stop short of the very end. The voice checks
                // `idx + 4 >= frames -> deactivate` BEFORE it checks the loop
                // wrap, because the 4-point interpolator needs that lookahead.
                // A loop point at `n - 1` is therefore unreachable: the voice
                // deactivates four samples before it can ever wrap, the deck
                // falls silent, and the run fails on "audio stopped part-way"
                // with looping apparently enabled.
                let end = n - LOOP_TAIL_GUARD;
                conductor.apply_mixer_commands(vec![Command::Performance(
                    PerformanceCommand::SetLoop { node_idx, enabled: true, start_samples: 0, end_samples: end },
                )]);
            }
            _ => eprintln!("WARN: deck {deck} could not be looped; a run longer than the track will report a false failure."),
        }
    }

    // Diagnostic: give the engine 3s, then ask the samplers what they hold.
    {
        let t0 = Instant::now();
        let mut last_tel = None;
        let mut next_probe = 0u64;
        while t0.elapsed() < Duration::from_secs(6) {
            conductor.tick();
            while let Some(mut tel) = context.telemetry_consumer.pop() {
                conductor.update_timeline(&mut tel);
                last_tel = Some(tel);
            }
            if t0.elapsed().as_secs() >= next_probe {
                next_probe += 1;
                let handle = conductor.engine_coordinator.backend_manager.engine_handle.lock();
                let children = handle.as_ref().map(|e| e.list_children().len()).unwrap_or(usize::MAX);
                if next_probe == 3 {
                    if let Some(e) = handle.as_ref() {
                        let types: Vec<(usize, &str, Option<u64>)> = e.list_children().iter().enumerate()
                            .map(|(i, c)| (i, c.processor_type(), c.resource_id())).take(12).collect();
                        eprintln!("PROBE types: {:?}", types);
                    }
                }
                let hot: Vec<usize> = last_tel.as_ref().map(|t| t.peak_levels.iter().enumerate().filter(|(_, p)| **p > 1e-6).map(|(i, _)| i).collect()).unwrap_or_default();
                eprintln!("PROBE t={}s children={} hot={:?}", t0.elapsed().as_secs(), children, hot);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        let handle = conductor.engine_coordinator.backend_manager.engine_handle.lock();
        if let Some(engine) = handle.as_ref() {
            for child in engine.list_children() {
                if child.processor_type() == "sampler" {
                    println!("DIAG: sampler resource_id={:?} playhead={}",
                        child.resource_id(), child.get_playback_position());
                }
            }
        }
        drop(handle);
        if let Some(tel) = last_tel.as_ref() {
            let hot: Vec<(usize, f32)> = tel.peak_levels.iter().enumerate()
                .filter(|(_, p)| **p > 1e-6).map(|(i, p)| (i, *p)).collect();
            println!("DIAG: node peaks (nonzero): {:?}", hot);
        }
        {
            let topo = &conductor.topology_manager.current_topology;
            for idx in 0..topo.node_count.min(64) {
                let r = &topo.routing[idx];
                let ins: Vec<u32> = r.input_indices[..r.input_count].iter().map(|b| b.0).collect();
                let outs: Vec<u32> = r.output_indices[..r.output_count].iter().map(|b| b.0).collect();
                println!("DIAG: node {:2} in={:?} out={:?}", idx, ins, outs);
            }
        }
    }

    // Which node is the master output. Everything downstream of it goes to the
    // device, so this is the only peak that says anything about clipping.
    let master_node_idx: Option<usize> = conductor
        .mixer_manager
        .node_names
        .get("master_limiter")
        .map(|&i| i as usize);
    if master_node_idx.is_none() {
        eprintln!("WARN: master_limiter node not found; master level will not be reported.");
    }

    // --- Main survival loop ---
    let run_duration = Duration::from_secs(args.minutes * 60);
    println!("Running for {} minute(s)...\n", args.minutes);
    let started = Instant::now();
    let mut stats = Stats::default();
    let mut last_xrun_count = 0u32;
    let mut last_progress = Instant::now();
    let budget_ns: u64 = {
        let cfg_budget = std::fs::read_to_string("system_config.json")
            .ok()
            .and_then(|c| serde_json::from_str::<nullherz_conductor::persistence::SystemConfig>(&c).ok())
            .map(|cfg| (cfg.period_size as f64 / cfg.sample_rate.max(1) as f64 * 1e9) as u64);
        cfg_budget.unwrap_or(0)
    };
    // A silent telemetry stream means the audio thread is dead (e.g. an RT
    // panic) — that must read as FAIL, never as a quiet PASS.
    let mut last_frame_at = Instant::now();

    let mut warm = false;
    while started.elapsed() < run_duration {
        // Latch the underrun count once the machine has settled. Everything
        // before this is startup transient and is reported separately.
        if !warm && started.elapsed() >= WARMUP {
            warm = true;
            stats.warmup_xruns = conductor
                .engine_coordinator
                .backend_manager
                .xruns()
                .unwrap_or(0);
            if stats.warmup_xruns > 0 {
                println!("[warm] {} underrun(s) during the first {}s, excluded from the verdict",
                         stats.warmup_xruns, WARMUP.as_secs());
            }
        }
        if last_frame_at.elapsed() > Duration::from_secs(10) {
            eprintln!(
                "FATAL: no telemetry for 10s — the audio thread has stopped (panic or stall). \
                 {} frames seen before silence.",
                stats.frames
            );
            std::process::exit(1);
        }
        conductor.tick();
        while let Some(mut tel) = context.telemetry_consumer.pop() {
            last_frame_at = Instant::now();
            conductor.update_timeline(&mut tel);
            stats.frames += 1;
            stats.sample_rate = tel.sample_rate;
            stats.samples_processed = tel.sample_counter;
            stats.sum_process_time_ns += tel.process_time_ns;
            stats.peak_process_time_ns = stats.peak_process_time_ns.max(tel.peak_process_time_ns);
            if budget_ns > 0 && tel.process_time_ns > budget_ns {
                stats.overrun_count += 1;
                if stats.overrun_events.len() < 64 {
                    stats.overrun_events.push((started.elapsed(), tel.process_time_ns));
                }
            }
            stats.resource_leaks_final = tel.resource_leaks;
            let block_peak = tel.peak_levels.iter().copied().fold(0.0f32, f32::max);
            if block_peak > 1e-6 {
                stats.frames_with_signal += 1;
            }
            stats.peak_output_level = stats.peak_output_level.max(block_peak);
            if let Some(idx) = master_node_idx
                && let Some(p) = tel.peak_levels.get(idx) {
                stats.peak_master_level = stats.peak_master_level.max(*p);
            }
            if tel.xrun_count != last_xrun_count {
                let elapsed = started.elapsed();
                println!(
                    "!! XRUN #{} at {:>6.1}s (magnitude {} ns)",
                    tel.xrun_count,
                    elapsed.as_secs_f64(),
                    tel.last_xrun_magnitude_ns
                );
                stats.xrun_events.push((elapsed, tel.xrun_count, tel.last_xrun_magnitude_ns));
                last_xrun_count = tel.xrun_count;
            }
            stats.xrun_count_final = tel.xrun_count;
        }

        if last_progress.elapsed() >= Duration::from_secs(60) {
            let mins = started.elapsed().as_secs() / 60;
            println!(
                "[{:>3} min] xruns: {}  peak block: {} us  frames: {}",
                mins,
                stats.xrun_count_final,
                stats.peak_process_time_ns / 1000,
                stats.frames
            );
            last_progress = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(16));
    }

    // --- Report ---
    // Sample the backend's own underrun counter before anything tears down,
    // discounting anything that happened while the machine was still settling.
    stats.backend_xruns = conductor
        .engine_coordinator
        .backend_manager
        .xruns()
        .map(|n| n.saturating_sub(stats.warmup_xruns));
    let elapsed = started.elapsed();
    let mean_block_us = if stats.frames > 0 { stats.sum_process_time_ns / stats.frames / 1000 } else { 0 };
    // DSP headroom: peak block time vs the period budget implied by the config.
    let period_budget_us = std::fs::read_to_string("system_config.json")
        .ok()
        .and_then(|c| serde_json::from_str::<nullherz_conductor::persistence::SystemConfig>(&c).ok())
        .map(|cfg| (cfg.period_size as f64 / stats.sample_rate.max(1.0) as f64 * 1_000_000.0) as u64)
        .unwrap_or(0);
    // A run is only meaningful if audio actually flowed. Require signal in a
    // solid majority of frames, not merely at some point: a deck that stops
    // early (voice deactivating at buffer end, a sampler wedging) would
    // otherwise leave the remainder silent and still pass on its first second.
    let signal_ratio = if stats.frames > 0 {
        stats.frames_with_signal as f64 / stats.frames as f64
    } else {
        0.0
    };
    let audio_flowed = stats.peak_output_level > 1e-6 && signal_ratio > 0.90;

    // Grade on the BACKEND's counter. `telemetry.xrun_count` is plumbed from an
    // atomic in audio-core that nothing ever increments, so the old criterion
    // `xrun_count_final == 0` was true no matter what happened — including the
    // runs where the threaded backend printed "Total Xruns: 13" to stderr while
    // this report said zero. A gate that cannot fail is not a gate.
    let xruns = stats.backend_xruns;
    let xruns_clean = matches!(xruns, Some(0));
    let pass = stats.frames > 0 && audio_flowed && xruns_clean;

    match xruns {
        None => eprintln!(
            "FATAL: the {backend:?} backend does not report underruns, so this run cannot \
             demonstrate anything about xruns. Treating that as a failure rather than \
             silently passing."
        ),
        Some(n) if n > 0 => eprintln!(
            "FAIL: {n} underrun(s) reported by the {backend:?} backend."
        ),
        Some(_) => {}
    }
    if stats.frames == 0 {
        eprintln!("FATAL: zero telemetry frames received — the audio thread never ran.");
    } else if stats.peak_output_level <= 1e-6 {
        eprintln!(
            "FATAL: the graph was SILENT for the entire run (peak {:.2e}). Zero xruns \
             is meaningless without audio — this is a failed run, not a passed one.",
            stats.peak_output_level
        );
    } else if signal_ratio <= 0.90 {
        eprintln!(
            "FATAL: audio stopped part-way — only {:.1}% of frames carried signal. \
             Playback did not survive the run even though no xrun was reported.",
            signal_ratio * 100.0
        );
    }

    let report = format!(
        "# Survival Test Report\n\n\
        | Field | Value |\n| :-- | :-- |\n\
        | Date | {} |\n\
        | Backend | {:?} |\n\
        | Duration | {:.1} min |\n\
        | Sample rate | {} Hz |\n\
        | Samples processed | {} |\n\
        | Telemetry frames | {} |\n\
        | **Xruns (backend, after warm-up)** | **{}** |\n\
        | Xruns during warm-up (excluded) | {} |\n\
        | Xruns (engine telemetry) | {} (counter is never incremented — see notes) |\n\
        | **Peak MASTER level** | **{:.4}** |\n\
        | Peak level, any node (pre-limiter; >1.0 is normal) | {:.4} |\n\
        | Frames with signal | {:.1}% |\n\
        | Peak block time | {} µs |\n\
        | Mean block time | {} µs |\n\
        | Period budget | {} µs |\n\
        | Resource leaks | {} |\n\
        | **Result** | **{}** |\n\n{}",
        chrono_free_timestamp(),
        backend,
        elapsed.as_secs_f64() / 60.0,
        stats.sample_rate,
        stats.samples_processed,
        stats.frames,
        match xruns { Some(n) => n.to_string(), None => "NOT REPORTED".to_string() },
        stats.warmup_xruns,
        stats.xrun_count_final,
        stats.peak_master_level,
        stats.peak_output_level,
        signal_ratio * 100.0,
        stats.peak_process_time_ns / 1000,
        mean_block_us,
        period_budget_us,
        stats.resource_leaks_final,
        if pass { "PASS" } else { "FAIL" },
        {
            let mut s = String::new();
            if !stats.xrun_events.is_empty() {
                s.push_str("## Xrun log\n\n| Elapsed (s) | Count | Magnitude (ns) |\n| --: | --: | --: |\n");
                for (at, count, mag) in &stats.xrun_events {
                    s.push_str(&format!("| {:.1} | {} | {} |\n", at.as_secs_f64(), count, mag));
                }
            }
            if !stats.overrun_events.is_empty() {
                s.push_str(&format!(
                    "\n## Budget overruns ({} total, first {} shown)\n\n| Elapsed (s) | Block time (µs) |\n| --: | --: |\n",
                    stats.overrun_count,
                    stats.overrun_events.len()
                ));
                for (at, ns) in &stats.overrun_events {
                    s.push_str(&format!("| {:.2} | {} |\n", at.as_secs_f64(), ns / 1000));
                }
            }
            s
        }
    );

    let report_path = args.report_path.unwrap_or_else(|| {
        format!("survival_report_{:?}_{}min.md", backend, args.minutes).to_lowercase()
    });
    if let Err(e) = std::fs::write(&report_path, &report) {
        eprintln!("Could not write report to {}: {}", report_path, e);
    } else {
        println!("\nReport written to {}", report_path);
    }

    if period_budget_us > 0 && stats.peak_process_time_ns / 1000 > period_budget_us {
        println!(
            "\nWARNING: peak block time ({} µs) exceeded the period budget ({} µs). \
             On ALSA/PipeWire this would have been an audible dropout; the Threaded \
             backend cannot detect it as an xrun. Treat a PASS here as provisional.",
            stats.peak_process_time_ns / 1000,
            period_budget_us
        );
    }
    println!(
        "\n=== {} — {} xrun(s) in {:.1} min on {:?} (peak block {} µs / budget {} µs) ===",
        if pass { "PASS" } else { "FAIL" },
        // The backend's count, same as the verdict. Printing the engine
        // telemetry counter here produced the contradiction this whole fix is
        // about: a headline "0 xrun(s)" next to a FAIL verdict.
        match xruns { Some(n) => n.to_string(), None => "unreported".to_string() },
        elapsed.as_secs_f64() / 60.0,
        backend,
        stats.peak_process_time_ns / 1000,
        period_budget_us,
    );
    std::process::exit(if pass { 0 } else { 1 });
}

/// RFC3339-ish local timestamp without adding a chrono dependency.
fn chrono_free_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("unix:{}", now.as_secs())
}
