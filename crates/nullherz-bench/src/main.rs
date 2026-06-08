use audio_core::{AudioEngine, ProcessorGraph, ThreadedBackend, AudioBackend};
use ipc_layer::RingBuffer;
use fx_runtime::SidecarManager;
use std::time::{Duration, Instant};

fn main() {
    println!("Starting nullherz-bench Phase 3 Stress Test...");

    let cmd_cons = std::sync::Arc::new(ipc_layer::MpscRingBuffer::new(1024));
    let mut cmd_prod = cmd_cons.clone();
    let (garbage_prod, _garbage_cons) = RingBuffer::new(1024).split();
    let (tel_prod, mut tel_cons) = RingBuffer::new(1024).split();

    let graph = ProcessorGraph::new();
    let mut manager = SidecarManager::new();

    println!("Spawning 16 SIMD-heavy sidecar nodes...");
    let dummy_path = "./target/debug/nullherz-sidecar-dummy";

    let mut graph = graph;
    for i in 0..16 {
        match manager.spawn_sidecar(&format!("bench_node_{}", i), dummy_path, 2) {
            Ok(processor) => {
                println!("Spawned node {}", i);
                graph.add_node(Box::new(processor), vec![], vec![i*2, i*2+1]);
            }
            Err(e) => println!("Failed to spawn node {}: {}", i, e),
        }
    }

    let mut engine = AudioEngine::new(cmd_cons, None, None, garbage_prod, None, tel_prod, Box::new(graph));

    let mut backends: Vec<Box<dyn AudioBackend>> = vec![
        Box::new(ThreadedBackend::new()),
        // ALSA/PipeWire/JACK might fail/segfault in sandbox if drivers/daemons are missing
        // Box::new(AlsaBackend::new()),
        // Box::new(PipewireBackend::new()),
        // Box::new(JackBackend::new()),
    ];

    let mut current_backend_idx = 0;
    let start_time = Instant::now();

    while start_time.elapsed() < Duration::from_secs(30) {
        let backend = &mut backends[current_backend_idx];
        println!("Switching to backend {}...", current_backend_idx);

        if let Ok(_) = backend.start(engine) {
            // Run Command Fuzzer during playback
            let fuzzer_start = Instant::now();
            let mut i = 0;
            while fuzzer_start.elapsed() < Duration::from_secs(5) {
                // Fuzz parameter updates
                let _ = cmd_prod.push(control_plane::TimestampedCommand {
                    timestamp_samples: 0,
                    command: control_plane::Command::SetParam {
                        target_id: (i % 16) as u64,
                        param_id: 0,
                        value: (i as f32 * 0.1).sin(),
                        ramp_duration_samples: 128,
                    },
                });

                // Fuzz edge updates (topology stress)
                let _ = cmd_prod.push(control_plane::TimestampedCommand {
                    timestamp_samples: 0,
                    command: control_plane::Command::UpdateEdgeCrossfaded {
                        node_idx: (i % 64) as u32,
                        input_idx: 0,
                        new_buffer_idx: ((i + 1) % 64) as u32,
                        duration_samples: 512,
                    },
                });

                i += 1;
                if i % 100 == 0 { std::thread::sleep(Duration::from_millis(1)); }
            }

            // Check telemetry
            while let Some(telemetry) = tel_cons.pop() {
                if telemetry.xrun_count > 0 {
                    println!("XRUN DETECTED: {}", telemetry.xrun_count);
                }
            }

            engine = backend.stop().expect("Backend should return engine");
        } else {
            println!("Failed to start backend {}. It might not be available on this system.", current_backend_idx);
            // Re-create engine from previous if possible, but we already moved it.
            // For bench, we just stop.
            break;
        }

        current_backend_idx = (current_backend_idx + 1) % backends.len();
    }

    println!("Stress test completed.");
}
