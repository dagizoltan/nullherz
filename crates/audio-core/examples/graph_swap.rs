use audio_core::{AudioEngine, AudioProcessor, ProcessorChain};
use control_plane::{TimestampedCommand};
use ipc_layer::{RingBuffer};

fn main() {
    let rb = RingBuffer::new(1024);
    let (_, cons) = rb.split();

    // We need a garbage producer/consumer for the engine
    let garbage_rb = RingBuffer::new(32);
    let (mut garbage_prod, mut _garbage_cons) = garbage_rb.split();

    let initial_graph = Box::new(ProcessorChain::new());
    let engine = AudioEngine::new(cons, garbage_prod, initial_graph);

    println!("Engine initialized with empty graph.");

    let mut new_graph = Box::new(ProcessorChain::new());
    // Add some processors to new_graph...

    println!("Swapping to new graph...");
    let old_graph = engine.swap_graph(new_graph);
    println!("Old graph recovered for deallocation.");
    drop(old_graph);

    println!("Simulation finished.");
}
