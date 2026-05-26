use audio_core::{AudioEngine, ProcessorChain};
use ipc_layer::{RingBuffer};

fn main() {
    let rb = RingBuffer::new(1024);
    let (_, cons) = rb.split();

    let garbage_rb = RingBuffer::new(32);
    let (garbage_prod, _) = garbage_rb.split();

    let initial_graph = Box::new(ProcessorChain::new());
    let mut engine = AudioEngine::new(cons, garbage_prod, initial_graph);

    println!("Engine initialized with empty graph.");

    let new_graph = Box::new(ProcessorChain::new());

    println!("Requesting graph swap...");
    engine.request_swap(new_graph);

    // In a real system, the swap happens when process_block is called.
    let mut out_buffer = [0.0f32; 128];
    let mut out_ptrs = [&mut out_buffer[..]];
    engine.process_block(&[], &mut out_ptrs, 128);

    println!("Graph swap should be completed by engine.");

    println!("Simulation finished.");
}
