use std::sync::Arc;
use ipc_layer::{RingBuffer, MpscRingBuffer};
use nullherz_traits::{AudioProcessor};
use audio_core::{AudioEngine, ProcessorGraph};

fn main() {
    let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();
    let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
    let cmd_buffer = Arc::new(MpscRingBuffer::new(1024));

    let graph = ProcessorGraph::new();

    let engine = AudioEngine::new(
        Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
        Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
        None, None, None, garbage_prod, None, None, None,
        Box::new(tel_prod),
        Box::new(graph)
    );

    println!("Engine created. Swapping graphs...");

    let new_graph = ProcessorGraph::new();
    engine.set_pending_graph(Box::new(new_graph));

    println!("Graph swap scheduled.");
}
