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
                track_ids = tracks.iter().map(|t| t.id).collect();
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

    println!("Loading track {} -> Deck A, track {} -> Deck B; starting playback.", track_ids[0], track_ids[1]);
    use nullherz_traits::{Command, PerformanceCommand};
    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: track_ids[0] }),
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'B', sample_id: track_ids[1] }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'B' }),
    ]);

    // --- Main survival loop ---
    let run_duration = Duration::from_secs(args.minutes * 60);
    println!("Running for {} minute(s)...\n", args.minutes);
    let started = Instant::now();
    let mut stats = Stats::default();
    let mut last_xrun_count = 0u32;
    let mut last_progress = Instant::now();
    // A silent telemetry stream means the audio thread is dead (e.g. an RT
    // panic) — that must read as FAIL, never as a quiet PASS.
    let mut last_frame_at = Instant::now();

    while started.elapsed() < run_duration {
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
            stats.resource_leaks_final = tel.resource_leaks;
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
    let elapsed = started.elapsed();
    let mean_block_us = if stats.frames > 0 { stats.sum_process_time_ns / stats.frames / 1000 } else { 0 };
    // DSP headroom: peak block time vs the period budget implied by the config.
    let period_budget_us = std::fs::read_to_string("system_config.json")
        .ok()
        .and_then(|c| serde_json::from_str::<nullherz_conductor::persistence::SystemConfig>(&c).ok())
        .map(|cfg| (cfg.period_size as f64 / stats.sample_rate.max(1.0) as f64 * 1_000_000.0) as u64)
        .unwrap_or(0);
    let pass = stats.xrun_count_final == 0 && stats.frames > 0;
    if stats.frames == 0 {
        eprintln!("FATAL: zero telemetry frames received — the audio thread never ran.");
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
        | **Xruns** | **{}** |\n\
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
        stats.xrun_count_final,
        stats.peak_process_time_ns / 1000,
        mean_block_us,
        period_budget_us,
        stats.resource_leaks_final,
        if pass { "PASS" } else { "FAIL" },
        if stats.xrun_events.is_empty() {
            String::new()
        } else {
            let mut s = String::from("## Xrun log\n\n| Elapsed (s) | Count | Magnitude (ns) |\n| --: | --: | --: |\n");
            for (at, count, mag) in &stats.xrun_events {
                s.push_str(&format!("| {:.1} | {} | {} |\n", at.as_secs_f64(), count, mag));
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
        stats.xrun_count_final,
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
