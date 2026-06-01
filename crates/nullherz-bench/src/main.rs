use audio_core::{AudioEngine, ProcessorGraph, ThreadedBackend, AudioBackend, SimdBiquadProcessor};
use ipc_layer::RingBuffer;
use std::time::Duration;

fn main() {
    println!("Starting nullherz stress test...");

    let rb = RingBuffer::new(1024);
    let (_, cons) = rb.split();
    let garbage_rb = RingBuffer::new(32);
    let (garbage_prod, _) = garbage_rb.split();
    let tel_rb = RingBuffer::new(1024);
    let (tel_prod, mut tel_cons) = tel_rb.split();

    let mut graph = ProcessorGraph::new();

    // Instantiate 16 SIMD-heavy sidecar nodes (simulated as internal SIMD processors for bench)
    for i in 0..16 {
        let coeffs = audio_dsp::BiquadCoefficients { b0: 0.1, b1: 0.2, b2: 0.1, a1: -0.5, a2: 0.2 };
        graph.add_node(Box::new(SimdBiquadProcessor::new(coeffs)), vec![i], vec![i+1]);
    }

    let mut engine = Some(AudioEngine::new(cons, garbage_prod, tel_prod, Box::new(graph)));

    let backends: Vec<(&str, Box<dyn AudioBackend>)> = vec![
        ("Threaded", Box::new(ThreadedBackend::new())),
        ("Threaded2", Box::new(ThreadedBackend::new())),
        // ("ALSA", Box::new(AlsaBackend::new())),
        // ("PipeWire", Box::new(PipewireBackend::new())),
    ];

    for (name, mut backend) in backends.into_iter() {
        println!("Switching to backend {}...", name);
        if let Some(e) = engine.take() {
            println!("Starting backend {}...", name);
            if let Err(e) = backend.start(e) {
                println!("Backend {} failed to start: {}", name, e);
                // In a real test we might want to recover, but here we just continue with Threaded if Alsa/Pw fail
                // since they might not have real hardware/daemon in this environment.
                continue;
            }

            std::thread::sleep(Duration::from_secs(1));

            // Check telemetry
            while let Some(tel) = tel_cons.pop() {
                if tel.xrun_count > 0 {
                    println!("XRUN detected: {}", tel.xrun_count);
                }
            }

            engine = backend.stop();
        }
    }

    println!("Stress test completed.");
}
