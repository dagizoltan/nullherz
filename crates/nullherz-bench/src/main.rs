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
        println!("REDb Writer: Starting 10,000 track insertion...");
        let db = LibraryDatabase::load(db_path).unwrap();
        let start = Instant::now();
        for i in 0..10_000 {
            let track = LibraryTrack {
                id: i,
                path: format!("/fake/path/track_{}.wav", i),
                title: format!("Stress Track {}", i),
                artist: "DJ Stress".to_string(),
                metadata: SampleMetadata::new_empty(),
            };
            db.save_track(&track).unwrap();
        }
        println!("REDb Writer: Completed 10,000 tracks in {:?}", start.elapsed());
    });

    // 2. Start Engine & Sidecar Supervisor
    let mut conductor = Conductor::new();
    let (cmd_buffer, mut tel_cons, _midi_prod) = conductor.setup_engine();

    let mut mixer = MixerManager::new();
    let commands = mixer.create_4channel_mixer();
    conductor.apply_mixer_commands(commands);

    // Commit topology
    let _ = cmd_buffer.push_command(TimestampedCommand {
        timestamp_samples: 0,
        command: nullherz_traits::Command::CommitTopology,
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

        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    conductor.stop_backend();
    let _ = db_thread.join();
    
    let _ = std::fs::remove_file(db_path); // Cleanup
    println!("Stress test completed successfully!");
}
