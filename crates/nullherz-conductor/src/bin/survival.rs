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
    /// Run the console with the decks STOPPED — a control for "is the cost in
    /// the audio?".
    ///
    /// The stall being hunted executes for milliseconds with the kernel provably
    /// idle, which is what content-dependent DSP cost looks like: denormals in a
    /// decaying tail, a limiter's look-ahead window doing more work on one
    /// envelope shape than another. If the spike still appears with no audio
    /// playing, the content hypothesis is dead and the cost is structural.
    ///
    /// This is a DIAGNOSTIC mode, never a passing run: the harness's own rule is
    /// that zero xruns means nothing without audio, and that rule is not relaxed
    /// so much as declared inapplicable — the verdict is printed as
    /// CONTROL rather than PASS.
    silence: bool,
    report_path: Option<String>,
}

fn parse_args() -> Args {
    let mut args = Args { minutes: 60, backend: None, tracks_dir: "tracks".to_string(), report_path: None, silence: false };
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
            "--silence" => args.silence = true,
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

/// What one troubled window looked like.
struct StallRow {
    at: Duration,
    /// Longest block the engine reported in this window.
    block_ns: u64,
    /// Audio-thread kernel counters over the window.
    counters: ipc_layer::thread_stats::ThreadCounters,
    /// How far the audio clock fell behind the wall clock across the window.
    ///
    /// The measurement that matters most, and the one that was missing. The
    /// engine's `process_time_ns` covers time INSIDE the callback only. A
    /// 9-minute run at 32 frames failed with an xrun while the peak block was
    /// 372 us against a 666 us budget — every block comfortably fast, and the
    /// device still underran. Time like that is lost BETWEEN callbacks, where no
    /// block timer can see it, and the only way to observe it is to compare
    /// samples actually produced against samples that should have been.
    clock_deficit_ms: f64,
    /// Why this row exists, for the report.
    trigger: &'static str,
    /// Sum of EVERY node's time in the offending block, in ns.
    ///
    /// `block_ns - node_sum_ns` is the engine's non-node overhead: routing,
    /// buffer copies, PDC input delays, mixing, and the first half of telemetry
    /// finalisation. Per-node counters cover only each processor's `process()`
    /// call, so a block whose nodes sum to 110 us out of 3050 us is not slow
    /// because of DSP — it is slow somewhere no node timer looks, and a top-4
    /// column alone cannot show that.
    node_sum_ns: u64,
    /// Deck playback positions, in samples, at the offending block.
    ///
    /// For the hypothesis that the cost is in the AUDIO, not the machine. If the
    /// spike lands at the same playback position on a second pass over the same
    /// track, it is the content at that point — which turns a once-per-half-hour
    /// ghost into a fixed input that can be extracted and replayed in a unit test.
    deck_positions: [u64; 4],
    /// The slowest graph nodes IN THE BLOCK THAT OVERRAN: (node index, ns).
    ///
    /// The engine already measures this per block and per node
    /// (`Telemetry::node_times_ns`, filled from `collect_node_times`); it was
    /// simply never surfaced at the moment a block went long. Without it a stall
    /// attributable to neither faults nor preemption reads "profile our code",
    /// which names the plane and not the processor. Captured at detection rather
    /// than at window end because the next telemetry frame has already
    /// overwritten it.
    top_nodes: Vec<(u32, u64)>,
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

    /// Peak block time AFTER warm-up.
    ///
    /// `peak_process_time_ns` above is a running max over the WHOLE run, warm-up
    /// included, while the xrun verdict deliberately excludes warm-up. Two
    /// headline numbers measured over different windows cannot be related to
    /// each other: the 2945 us peak that killed a 128-frame evaluation could not
    /// be attributed to the xrun at 18.8 s because one figure had seen the first
    /// five seconds and the other had not. Both are reported now.
    peak_warm_ns: u64,

    /// One row per window that showed trouble: a block over budget, an xrun, or
    /// the audio clock falling behind the wall clock.
    ///
    /// The whole point of the instrument. A ~3 ms stall against a 135 us mean is
    /// not audio work, and these say which KIND of non-audio event it was.
    stall_attribution: Vec<StallRow>,
    /// Counters summed over windows where NOTHING overran, and how many such
    /// windows there were — the baseline a stall window is read against.
    quiet_counters: ipc_layer::thread_stats::ThreadCounters,
    quiet_windows: u64,
    /// Set if the audio thread never published a tid, so the report says
    /// "not measured" instead of implying a quiet result.
    attribution_available: bool,
    /// CPUs the audio thread is actually allowed on, read back from the kernel.
    ///
    /// Reported because pinning that silently did not happen looks exactly like
    /// pinning that did. The Threaded backend used to pin to CPU 0
    /// unconditionally and nothing anywhere said so.
    audio_affinity: Option<String>,
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
    use nullherz_traits::{Command, PerformanceCommand};
    if args.silence {
        println!(
            "--silence: decks loaded but NOT started. The graph runs end to end on zeroes, so \
             any block cost that remains is structural and not the audio."
        );
    } else {
        println!("Loading track {} -> Deck A, track {} -> Deck B; starting playback.", track_ids[0], track_ids[1]);
    }
    let mut startup: Vec<Command> = vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: track_ids[0] }),
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'B', sample_id: track_ids[1] }),
    ];
    if !args.silence {
        startup.push(Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }));
        startup.push(Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'B' }));
    }
    conductor.apply_mixer_commands(startup);

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
    // ONE period, honouring NULLHERZ_PERIOD_SIZE, shared by the overrun detector
    // here and the report below.
    //
    // These were computed separately and only the report honoured the override.
    // So during every period-ladder evaluation the overrun detector compared
    // against the CONFIG budget — 5333 us while the real one was 1333 — and
    // recorded nothing, while the report warned that the budget had been blown.
    // A six-minute run at 64 frames FAILED with two xruns and a 2197 us peak and
    // still printed "No block overran the period budget": `overrun_events` and
    // `overrun_count` were silently empty through the whole period sweep that
    // set the current default.
    let (cfg_period, cfg_rate) = std::fs::read_to_string("system_config.json")
        .ok()
        .and_then(|c| serde_json::from_str::<nullherz_conductor::persistence::SystemConfig>(&c).ok())
        .map(|cfg| (cfg.period_size, cfg.sample_rate))
        .unwrap_or((0, 0));
    let effective_period: u64 = std::env::var("NULLHERZ_PERIOD_SIZE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(cfg_period);
    // Seeded from the config rate because telemetry has not arrived yet; the
    // device rate replaces it on the first frame.
    let mut budget_ns: u64 = if effective_period > 0 && cfg_rate > 0 {
        (effective_period as f64 / cfg_rate as f64 * 1e9) as u64
    } else {
        0
    };
    // A silent telemetry stream means the audio thread is dead (e.g. an RT
    // panic) — that must read as FAIL, never as a quiet PASS.
    let mut last_frame_at = Instant::now();

    let mut warm = false;

    // Audio-thread kernel counters, sampled from THIS thread. Deliberately not
    // from the audio thread: a `getrusage` per block would put a syscall in the
    // hot path in order to measure whether the hot path is being interrupted by
    // the kernel, at ~1% of a 135 us budget. Reading /proc from the monitor
    // costs the audio thread nothing; the price is that an event is located to a
    // poll window rather than to a block, which is enough to identify its kind.
    let mut prev_counters = ipc_layer::thread_stats::audio_thread_tid()
        .and_then(ipc_layer::thread_stats::read_thread_counters);
    stats.attribution_available = prev_counters.is_some();
    stats.audio_affinity = ipc_layer::thread_stats::audio_thread_tid()
        .and_then(ipc_layer::thread_stats::thread_cpu_affinity);

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
        let overruns_at_window_start = stats.overrun_count;
        let xruns_at_window_start = stats.xrun_count_final;
        let samples_at_window_start = stats.samples_processed;
        let window_started = Instant::now();
        let mut window_peak_block_ns = 0u64;
        // Node times belong to ONE block, so they have to be taken from the
        // frame that overran, not reconstructed afterwards.
        let mut window_top_nodes: Vec<(u32, u64)> = Vec::new();
        let mut window_deck_positions = [0u64; 4];
        let mut window_node_sum = 0u64;
        while let Some(mut tel) = context.telemetry_consumer.pop() {
            last_frame_at = Instant::now();
            conductor.update_timeline(&mut tel);
            stats.frames += 1;
            stats.sample_rate = tel.sample_rate;
            // The device rate is authoritative and only observable once frames
            // flow — the config may say 44100 while ALSA negotiated 48000, which
            // is a 9% error on the budget every comparison below depends on.
            if tel.sample_rate > 0.0 && effective_period > 0 {
                budget_ns = (effective_period as f64 / tel.sample_rate as f64 * 1e9) as u64;
            }
            stats.samples_processed = tel.sample_counter;
            stats.sum_process_time_ns += tel.process_time_ns;
            stats.peak_process_time_ns = stats.peak_process_time_ns.max(tel.peak_process_time_ns);
            // `tel.process_time_ns` is THIS block, not a running max, so the
            // warm peak has to be accumulated from it rather than from
            // `peak_process_time_ns` — which already carries warm-up inside it
            // and can never be un-mixed.
            if warm {
                stats.peak_warm_ns = stats.peak_warm_ns.max(tel.process_time_ns);
            }
            if tel.process_time_ns > window_peak_block_ns {
                window_peak_block_ns = tel.process_time_ns;
                // Snapshot unconditionally on a new window peak: an outlier row
                // is decided at window end, by which point this frame is gone.
                let mut ranked: Vec<(u32, u64)> = tel
                    .node_times_ns
                    .iter()
                    .enumerate()
                    .filter(|(_, ns)| **ns > 0)
                    .map(|(i, ns)| (i as u32, *ns))
                    .collect();
                ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1));
                ranked.truncate(4);
                window_top_nodes = ranked;
                window_deck_positions = tel.deck_positions;
                window_node_sum = tel.node_times_ns.iter().sum();
            }
            if budget_ns > 0 && tel.process_time_ns > budget_ns {
                stats.overrun_count += 1;
                let mut ranked: Vec<(u32, u64)> = tel
                    .node_times_ns
                    .iter()
                    .enumerate()
                    .filter(|(_, ns)| **ns > 0)
                    .map(|(i, ns)| (i as u32, *ns))
                    .collect();
                ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1));
                ranked.truncate(4);
                window_top_nodes = ranked;
                window_deck_positions = tel.deck_positions;
                window_node_sum = tel.node_times_ns.iter().sum();
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
        // Close the window: decide whether it showed trouble, and either
        // attribute it or bank it as baseline. A stall window's "4200 minor
        // faults" has nothing to be large compared to without the baseline — the
        // same reason every THD number in this tree is read against an analyser
        // floor.
        //
        // Three independent triggers, because they catch different failures.
        // Block-over-budget catches slow DSP. Xrun catches what the backend saw.
        // Clock deficit catches time lost BETWEEN callbacks, which neither of the
        // others can see and which is what actually failed the 32-frame run.
        let wall_ms = window_started.elapsed().as_secs_f64() * 1000.0;
        let deficit_ms = if stats.sample_rate > 0.0 && stats.samples_processed >= samples_at_window_start {
            let produced = (stats.samples_processed - samples_at_window_start) as f64;
            let produced_ms = produced / stats.sample_rate as f64 * 1000.0;
            (wall_ms - produced_ms).max(0.0)
        } else {
            0.0
        };
        // A third of a window is far above sampling jitter and far below the
        // milliseconds an underrun costs.
        let clock_slipped = warm && deficit_ms > wall_ms / 3.0;
        // A block can be pathological without exceeding the period budget: at
        // 256 frames the budget is 5333 us and a 3700 us block — ten times the
        // mean — sails through. Waiting for a budget breach means only the very
        // worst events are ever attributed, and at 64 frames that was one event
        // in half an hour. Anything at 4x the run's own mean is worth a row.
        let mean_so_far = if stats.frames > 0 { stats.sum_process_time_ns / stats.frames } else { 0 };
        let outlier = warm
            && mean_so_far > 0
            && window_peak_block_ns > mean_so_far * 4
            // Floor: early in a run the mean is tiny and everything is 4x it.
            && window_peak_block_ns > 250_000;
        let trigger = if stats.overrun_count != overruns_at_window_start {
            Some("block over budget")
        } else if stats.xrun_count_final != xruns_at_window_start {
            Some("xrun, block within budget")
        } else if clock_slipped {
            Some("audio clock fell behind")
        } else if outlier {
            Some("outlier vs mean")
        } else {
            None
        };

        if let Some(now) = ipc_layer::thread_stats::audio_thread_tid()
            .and_then(ipc_layer::thread_stats::read_thread_counters)
        {
            if let Some(prev) = prev_counters {
                let d = now.since(&prev);
                match trigger {
                    Some(trigger) => {
                        if stats.stall_attribution.len() < 64 {
                            stats.stall_attribution.push(StallRow {
                                at: started.elapsed(),
                                block_ns: window_peak_block_ns,
                                counters: d,
                                clock_deficit_ms: deficit_ms,
                                trigger,
                                top_nodes: std::mem::take(&mut window_top_nodes),
                                deck_positions: window_deck_positions,
                                node_sum_ns: window_node_sum,
                            });
                        }
                    }
                    None => {
                        stats.quiet_counters.minor_faults += d.minor_faults;
                        stats.quiet_counters.major_faults += d.major_faults;
                        stats.quiet_counters.involuntary_switches += d.involuntary_switches;
                        stats.quiet_counters.voluntary_switches += d.voluntary_switches;
                        stats.quiet_windows += 1;
                    }
                }
            }
            prev_counters = Some(now);
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
    // DSP headroom: peak block time vs the period budget.
    //
    // NULLHERZ_PERIOD_SIZE must be honoured here or the whole column lies. This
    // read the saved `system_config.json` alone, so an evaluation run at 64
    // frames still reported the 5333 us budget of the saved 256 — every
    // "peak block X / budget 5333" line during the period sweep was wrong by 4x,
    // and a run at 98% of its real budget read as 25%.
    //
    // The backend's own "[ALSA] Negotiated: ..." line remains the authority:
    // `snd_pcm_hw_params_set_period_size_near` can return something other than
    // what was asked, and nothing here can see that.
    let period = effective_period;
    let period_budget_us = if period > 0 {
        (period as f64 / stats.sample_rate.max(1.0) as f64 * 1_000_000.0) as u64
    } else {
        0
    };
    // A run is only meaningful if audio actually flowed. Require signal in a
    // solid majority of frames, not merely at some point: a deck that stops
    // early (voice deactivating at buffer end, a sampler wedging) would
    // otherwise leave the remainder silent and still pass on its first second.
    let signal_ratio = if stats.frames > 0 {
        stats.frames_with_signal as f64 / stats.frames as f64
    } else {
        0.0
    };
    // In --silence the decks were never started, so "no audio" is the intended
    // condition rather than a failure. The rule is not relaxed, it is declared
    // inapplicable: the verdict prints CONTROL, never PASS, so nobody can cite a
    // silence run as evidence the console survives playback.
    let audio_flowed = args.silence || (stats.peak_output_level > 1e-6 && signal_ratio > 0.90);

    // Grade on the BACKEND's counter. `telemetry.xrun_count` is plumbed from an
    // atomic in audio-core that nothing ever increments, so the old criterion
    // `xrun_count_final == 0` was true no matter what happened — including the
    // runs where the threaded backend printed "Total Xruns: 13" to stderr while
    // this report said zero. A gate that cannot fail is not a gate.
    let xruns = stats.backend_xruns;
    let xruns_clean = matches!(xruns, Some(0));
    // HEADROOM is a pass criterion, not a footnote.
    //
    // `overrun_count` — blocks whose process time exceeded the device period —
    // was collected, printed, and then ignored by the verdict, which turned on
    // xruns and signal presence alone. That leaves the one failure mode the
    // Threaded backend cannot report (it has no xrun counter) invisible: a
    // console that overran its budget on 5% of blocks passed clean, and the
    // problem only appeared as crackle on someone's real hardware.
    //
    // Rate, not peak. A single outlier is scheduler noise on any machine
    // without core isolation — a 4-deck console measured 136 µs mean against a
    // 5,805 µs budget and still produced one 3,376 µs block in 20,000. What
    // matters is whether overruns are RARE, so the gate is a fraction of all
    // blocks. 0.1% is the p99.9 the architecture notes argue is the binding
    // constraint in RT audio; override for a deliberately loaded run.
    let overrun_budget_ratio: f64 = std::env::var("NULLHERZ_MAX_OVERRUN_RATIO")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.001);
    let overrun_ratio = if stats.frames > 0 {
        stats.overrun_count as f64 / stats.frames as f64
    } else {
        0.0
    };
    let headroom_ok = overrun_ratio <= overrun_budget_ratio;
    if !headroom_ok {
        eprintln!(
            "FAIL: {} of {} blocks ({:.3}%) exceeded the {} µs period budget — over the \
             {:.3}% allowed. The run survived, but with no headroom: on hardware that \
             reports underruns these are dropouts. Peak block {} µs.",
            stats.overrun_count,
            stats.frames,
            overrun_ratio * 100.0,
            period_budget_us,
            overrun_budget_ratio * 100.0,
            stats.peak_process_time_ns / 1000,
        );
    }

    let pass = stats.frames > 0 && audio_flowed && xruns_clean && headroom_ok;

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
    } else if args.silence {
        // Expected: this is the control.
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

    // ---- stall attribution ------------------------------------------------
    //
    // The console's real-time ceiling is not DSP cost: mean block time runs
    // ~135 us against a 2666 us budget at 128 frames, while the peak is ~22x
    // that and arrives regardless of block size. So a block that overran did not
    // overrun doing audio work, and this says what it WAS doing. Faults point at
    // the memory architecture (mlockall is MCL_CURRENT only, so a growing heap
    // faults lazily); involuntary switches point at the scheduler, where
    // isolcpus is the answer; neither points at our own code.
    // node index -> processor name, for the attribution rows. Two hops:
    // `active_node_types` maps index to a processor TYPE id, and the registry
    // maps that id to a name.
    let node_name = {
        let reg = nullherz_processors::registry::ProcessorRegistry::new();
        let type_names: std::collections::HashMap<u32, String> = reg
            .list_available_processors()
            .into_iter()
            .map(|(id, name)| (id, name.to_string()))
            .collect();
        let types = conductor.topology_manager.active_node_types.clone();
        move |idx: u32| -> String {
            match types.get(&idx) {
                Some(tid) => match type_names.get(tid) {
                    Some(n) => format!("{idx}:{n}"),
                    None => format!("{idx}:type{tid}"),
                },
                None => format!("{idx}:?"),
            }
        }
    };

    let attribution = if !stats.attribution_available {
        "\n## Stall attribution\n\nNOT MEASURED — the audio thread never published a task id, so `ipc_layer::thread_stats` had nothing to sample. It publishes from `set_rt_priority`; a backend that never asks for real-time priority will not appear here.\n".to_string()
    } else if stats.stall_attribution.is_empty() {
        format!(
            "\n## Stall attribution\n\nNo block overran the period budget, no xrun landed, and the audio clock never fell behind, so there was nothing to attribute. Baseline over {} quiet windows: {} minor faults, {} major, {} involuntary switches, {} voluntary (the last is the proof the sampled thread really is blocking per block — a clean baseline means nothing if the thread was idle).\n",
            stats.quiet_windows,
            stats.quiet_counters.minor_faults,
            stats.quiet_counters.major_faults,
            stats.quiet_counters.involuntary_switches,
            stats.quiet_counters.voluntary_switches,
        )
    } else {
        let w = stats.quiet_windows.max(1);
        let q_minor = stats.quiet_counters.minor_faults as f64 / w as f64;
        let q_invol = stats.quiet_counters.involuntary_switches as f64 / w as f64;
        let mut t = format!(
            "\n## Stall attribution\n\nAudio-thread kernel counters over the ~16 ms window containing each overrun, sampled from the monitor thread (nothing is added to the audio thread). Read against the quiet baseline, not against zero: the sampler itself costs a couple of minor faults per window.\n\n| Baseline (per quiet window, n={}) | minor faults | major | involuntary sw | voluntary sw |\n| :-- | --: | --: | --: | --: |\n| mean | {:.2} | {:.3} | {:.3} | {:.1} |\n\n| Elapsed (s) | trigger | Block (µs) | all nodes (µs) | NOT in nodes (µs) | clock lost (ms) | minor flt | invol sw | reads as | slowest nodes (µs) | deck A pos |\n| --: | :-- | --: | --: | --: | --: | --: | --: | :-- | :-- | --: |\n",
            w,
            q_minor,
            stats.quiet_counters.major_faults as f64 / w as f64,
            q_invol,
            stats.quiet_counters.voluntary_switches as f64 / w as f64,
        );
        for row in stats.stall_attribution.iter().take(20) {
            let (at, block_ns, d) = (row.at, row.block_ns, row.counters);
            // Thresholds are deliberately blunt. This classifies, it does not
            // diagnose — the point is to say which of three very different fixes
            // to go and work on, and a 3 ms stall's cause shows up as orders of
            // magnitude, not as a marginal ratio.
            let faults_hot = d.minor_faults > ipc_layer::thread_stats::ATTRIBUTION_FLOOR_FAULTS
                && (d.minor_faults as f64) > q_minor * 4.0;
            let sched_hot = d.involuntary_switches > 0
                && (d.involuntary_switches as f64) > q_invol * 4.0 + 1.0;
            // Block-within-budget plus a clock deficit means the time went
            // missing OUTSIDE the callback: the thread was not called, or was not
            // called on time. That is a wakeup/scheduling problem, and no amount
            // of work on the DSP inside the callback touches it.
            let outside_callback = block_ns < budget_ns && row.clock_deficit_ms > 0.5;
            let reads = match (d.major_faults > 0, faults_hot, sched_hot, outside_callback) {
                (true, _, _, _) => "MAJOR FAULTS — disk or swap on the audio path",
                (_, true, true, _) => "faults AND preemption",
                (_, true, false, _) => "PAGE FAULTS — memory architecture",
                (_, false, true, _) => "PREEMPTION — scheduler / IRQ",
                (_, false, false, true) => "OUTSIDE THE CALLBACK — wakeup latency, not DSP",
                // Nodes accounting for under a third of the block means the cost
                // is not DSP at all: it is routing, buffer handling, PDC input
                // delays, mixing or telemetry — none of which a node timer covers.
                (_, false, false, false) if row.node_sum_ns * 3 < block_ns => {
                    "NOT IN ANY NODE — engine overhead outside DSP"
                }
                (_, false, false, false) => "unexplained — profile our code",
            };
            t.push_str(&format!(
                "| {:.1} | {} | {} | {} | **{}** | {:.2} | {} | {} | {} | {} | {} |\n",
                at.as_secs_f64(),
                row.trigger,
                block_ns / 1000,
                row.node_sum_ns / 1000,
                block_ns.saturating_sub(row.node_sum_ns) / 1000,
                row.clock_deficit_ms,
                d.minor_faults,
                d.involuntary_switches,
                reads,
                if row.top_nodes.is_empty() {
                    "not captured (trigger was not a block overrun)".to_string()
                } else {
                    row.top_nodes
                        .iter()
                        .map(|(i, ns)| format!("{} {}", node_name(*i), ns / 1000))
                        .collect::<Vec<_>>()
                        .join(", ")
                },
                row.deck_positions[0],
            ));
        }
        t
    };

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
        | Peak block time (whole run, warm-up included) | {} µs |\n\
        | **Peak block time (after warm-up)** | **{} µs** |\n\
        | Mean block time | {} µs |\n\
        | Period budget | {} µs |\n\
        | Audio thread CPUs (kernel-reported) | {} |\n\
        | Kernel CPU isolation (isolcpus/nohz_full) | {} |\n\
        | Denormal flushing (FTZ/DAZ) on the audio thread | {} |\n\
        | Resource leaks | {} |\n\
        | **Result** | **{}** |\n{}\n{}",
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
        stats.peak_warm_ns / 1000,
        mean_block_us,
        period_budget_us,
        match &stats.audio_affinity {
            Some(a) => a.clone(),
            None => "unknown (thread never identified itself)".to_string(),
        },
        if ipc_layer::has_isolated_cpus() { "present" } else { "NONE — pinning shares the CPU with everything else" },
        if ipc_layer::ftz_daz_was_lost() {
            "LOST DURING THE RUN — denormal arithmetic ran unflushed; this alone explains \
             multi-millisecond blocks on decaying audio"
        } else {
            "held for the whole run"
        },
        stats.resource_leaks_final,
        match (pass, args.silence) {
            (true, true) => "CONTROL (no audio — diagnostic only, never a pass)",
            (true, false) => "PASS",
            (false, _) => "FAIL",
        },
        attribution,
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
        match (pass, args.silence) {
            (true, true) => "CONTROL (silence)",
            (true, false) => "PASS",
            (false, _) => "FAIL",
        },
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
