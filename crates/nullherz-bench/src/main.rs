use audio_core::{AudioEngine, ProcessorGraph, ThreadedBackend, AlsaBackend, PipewireBackend, AudioBackend};
use ipc_layer::RingBuffer;
use fx_runtime::SidecarManager;
use std::time::{Duration, Instant};

fn main() {
    println!("Starting nullherz-bench Phase 3 Stress Test...");

    let (_cmd_prod, cmd_cons) = RingBuffer::new(1024).split();
    let (garbage_prod, _garbage_cons) = RingBuffer::new(1024).split();
    let (tel_prod, mut tel_cons) = RingBuffer::new(1024).split();

    let graph = ProcessorGraph::new();
    let mut manager = SidecarManager::new();

    println!("Spawning 16 SIMD-heavy sidecar nodes...");
    for i in 0..16 {
        // In a real bench, we'd have a dummy SIMD binary.
        // For now, we assume manager can attempt to spawn (it will fail to find binary but we test the logic).
        let _ = manager.spawn_sidecar(&format!("bench_node_{}", i), "./nullherz-sidecar-dummy", 2);
    }

    let mut engine = AudioEngine::new(cmd_cons, garbage_prod, tel_prod, Box::new(graph));

    let mut backends: Vec<Box<dyn AudioBackend>> = vec![
        Box::new(ThreadedBackend::new()),
        Box::new(AlsaBackend::new()),
        Box::new(PipewireBackend::new()),
    ];

    let mut current_backend_idx = 0;
    let start_time = Instant::now();

    while start_time.elapsed() < Duration::from_secs(30) {
        let backend = &mut backends[current_backend_idx];
        println!("Switching to backend {}...", current_backend_idx);

        if let Ok(_) = backend.start(engine) {
            std::thread::sleep(Duration::from_secs(5));

            // Check telemetry
            while let Some(telemetry) = tel_cons.pop() {
                if telemetry.xrun_count > 0 {
                    println!("XRUN DETECTED: {}", telemetry.xrun_count);
                }
            }

            engine = backend.stop().expect("Backend should return engine");
        } else {
            println!("Failed to start backend {}", current_backend_idx);
            // If it failed, we still need an engine for the next one, but how?
            // This is just a bench, so we assume success for the architecture test.
            break;
        }

        current_backend_idx = (current_backend_idx + 1) % backends.len();
    }

    println!("Stress test completed.");
}
