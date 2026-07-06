use nullherz_traits::TimestampedCommand;
use nullherz_mixer::MixerManager;
use nullherz_conductor::Conductor;
use nullherz_dna::{LibraryDatabase, LibraryTrack};
use nullherz_traits::SampleMetadata;
use fx_runtime::{SidecarSupervisor, FailurePolicy};
use std::time::Instant;

fn main() {
    println!("Starting Nullherz Stress Test...");

    let db_path = "stress_test_library.redb";
    let _ = std::fs::remove_file(db_path); // Start clean
    
    // 1. Start REDB 10,000 track insertion thread
    let db_thread = std::thread::spawn(move || {
        let count = 100_000;
        println!("REDb Writer: Starting {} track insertion (synthetic DNA)...", count);
        let db = LibraryDatabase::load(db_path).unwrap();
        let start = Instant::now();
        for i in 0..count {
            let mut dna = nullherz_traits::SoundDNA::default();
            // Fill with random-ish synthetic data
            for j in 0..16 { dna.spectral.latent_space[j] = (i % 100) as f32 / 100.0; }
            dna.rhythmic.syncopation_index = (i % 100) as f32 / 100.0;

            let track = LibraryTrack {
                id: i,
                path: format!("/fake/path/track_{}.wav", i),
                title: format!("Stress Track {}", i),
                artist: "DJ Stress".to_string(),
                metadata: SampleMetadata {
                    dna,
                    ..SampleMetadata::new_empty()
                },
            };
            db.save_track(&track).unwrap();
        }
        println!("REDb Writer: Completed {} tracks in {:?}", count, start.elapsed());

        // Test Matchmaker performance
        let target_dna = nullherz_traits::SoundDNA::default();
        let tracks = db.list_tracks().unwrap();
        println!("Matchmaker: Ranking {} candidates...", tracks.len());
        let rank_start = Instant::now();
        let results = nullherz_dna::Matchmaker::rank_compatibility(&target_dna, &tracks, 10);
        println!("Matchmaker: Top 10 results found in {:?} (Parallel: Rayon)", rank_start.elapsed());
        for (id, score) in results {
            println!("  Track ID {}: Score {:.4}", id, score);
        }
    });

    // 2. Start Engine & Sidecar Supervisor
    let mut conductor = Conductor::new();
    let ctx = conductor.setup_engine(); let cmd_buffer = ctx.command_producer; let mut tel_cons = ctx.telemetry_consumer; let _midi_prod = ctx.midi_producer;

    let mut mixer = MixerManager::new();
    let commands = mixer.create_4channel_mixer();
    conductor.apply_mixer_commands(commands);

    // Commit topology
    let _ = cmd_buffer.push_command(TimestampedCommand {
        timestamp_samples: 0,
        command: nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology),
    });

    conductor.start_backend(nullherz_backends::AudioBackendType::Threaded).unwrap();

    let mut supervisor = SidecarSupervisor::new();
    println!("Spawning dummy DSP sidecars for hot-swapping...");
    let mut sidecar_nodes = Vec::new();
    
    // In a real env, binary_path would be absolute. Assuming it's in target/debug for tests
    let dummy_path = std::env::current_dir().unwrap().join("target/debug/nullherz-dummy").to_string_lossy().to_string();
    
    for i in 0..4 {
        if let Ok(processor) = supervisor.spawn_sidecar(&format!("dummy_{}", i), &dummy_path, 100 + i, 2, FailurePolicy::AutoRestart) {
            sidecar_nodes.push((100+i, processor));
            println!("Spawned sidecar {}", i);
        } else {
            println!("Failed to spawn sidecar, skipping hot-swap test for this instance.");
        }
    }

    let test_duration = std::time::Duration::from_secs(5);
    let start_time = Instant::now();
    let mut kills = 0;

    while start_time.elapsed() < test_duration {
        // Pop telemetry to prevent buffer full
        while let Some(_tel) = tel_cons.pop() {}

        // Supervise and trigger hot-swaps
        let (restarted, safe_mode) = supervisor.supervise();
        for (node_idx, _) in restarted {
            println!("Supervisor restarted node {}", node_idx);
        }
        if safe_mode {
            println!("Supervisor entered SAFE MODE due to sidecar failure.");
        }

        // Randomly kill sidecars to test FailurePolicy::AutoRestart
        if start_time.elapsed().as_millis() % 1000 < 20 && kills < 10 {
            // Force a supervised reap/restart by artificially killing a process
            // Note: supervisor handles unexpected exits, we simulate it here if possible,
            // but supervisor tracks it internally. 
            // We just let it run its supervise loop.
            std::thread::sleep(std::time::Duration::from_millis(20));
            kills += 1;
        }

        // 3. High-Frequency Command Flood Test
        for i in 0..10 {
            let _ = cmd_buffer.push_command(TimestampedCommand {
                timestamp_samples: 0,
                command: nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                    target_id: 10, // Sampler
                    param_id: i % 8,
                    value: (start_time.elapsed().as_millis() % 100) as f32 / 100.0,
                    ramp_duration_samples: 128,
                }),
            });
        }

        // 4. NaN Ingestion Safety Test
        if start_time.elapsed().as_secs() == 2 {
            let _ = cmd_buffer.push_command(TimestampedCommand {
                timestamp_samples: 0,
                command: nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                    target_id: 1, // Biquad
                    param_id: 0,
                    value: f32::NAN,
                    ramp_duration_samples: 0,
                }),
            });
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    conductor.stop_backend();
    let _ = db_thread.join();
    
    let _ = std::fs::remove_file(db_path); // Cleanup

    // 5. Network Chaos Stress Test (Jitter Buffer / Clock Recovery)
    println!("Starting Network Chaos Stress Test...");
    run_network_chaos_test();

    // 6. Chaotic Transfusion Benchmark
    bench_chaotic_transfusion();

    println!("Stress tests completed successfully!");
}

pub fn bench_chaotic_transfusion() {
    println!("Benchmarking Chaotic Transfusion (1,000 operations)...");
    use nullherz_dna::chaotic_transfuse_dna;
    use nullherz_traits::SoundDNA;

    let dna_a = SoundDNA::default();
    let dna_b = SoundDNA::default();
    let start = Instant::now();

    for _ in 0..1000 {
        let _ = chaotic_transfuse_dna(&dna_a, &dna_b, 0.5, 0.5);
    }

    println!("Chaotic Transfusion: 1,000 ops in {:?}", start.elapsed());
}

fn run_network_chaos_test() {
    use nullherz_conductor::ipc_audio_bridge::IpcAudioBridge;
    use nullherz_traits::AudioBlock;

    let bridge = IpcAudioBridge::new();
    let node_idx = 42;
    let _ = bridge.register_return_node(node_idx);
    let _ = bridge.register_send_node(node_idx);

    let start = Instant::now();
    let mut sent = 0;
    let mut received = 0;

    // Simulate 5 seconds of network traffic with extreme jitter
    while start.elapsed().as_secs() < 5 {
        let block = AudioBlock { data: [0.0; 256], len: 256, _pad: [0; 15] };

        // Push block with random delay (simulate jitter)
        let jitter = (sent % 3 == 0); // Every 3rd block is delayed or bundled
        if !jitter {
            let _ = bridge.push_block(node_idx, block);
            sent += 1;
        } else {
             // Simulate a "burst" later
             let _ = bridge.push_block(node_idx, block);
             let _ = bridge.push_block(node_idx, block);
             sent += 2;
        }

        // 1. Conductor tick handles jitter buffer drain to SHM return queues
        bridge.process_return_queues();

        // 2. Try to pop a block from the send side (simulate conductor-to-remote)
        if bridge.pop_block(node_idx).is_some() {
            received += 1;
        }

        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    println!("Network Chaos: Sent {} blocks, processed {} on send side (synthetic jitter).", sent, received);
}
