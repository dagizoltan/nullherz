use audio_core::{AudioEngine, ProcessorGraph, ThreadedBackend, AudioBackend};
use ipc_layer::{RingBuffer};

fn main() {
    let rb = RingBuffer::new(1024);
    let (_, cons) = rb.split();
    let garbage_rb = RingBuffer::new(32);
    let (garbage_prod, _) = garbage_rb.split();
    let tel_rb = RingBuffer::new(1024);
    let (tel_prod, _) = tel_rb.split();

    let initial_graph = Box::new(ProcessorGraph::new());
    let mut engine = AudioEngine::new(cons, garbage_prod, tel_prod, initial_graph);

    println!("Engine initialized.");

    // Note: AudioEngine::request_swap was removed as it was not RT-safe or reachable after engine start.
    // Graph swaps should now be handled via the command path or a future TopologyBus.

    let mut backend = ThreadedBackend::new();
    backend.start(engine).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));
    println!("Simulation finished.");
    backend.stop();
}
