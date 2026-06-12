use audio_core::{AudioEngine, ProcessorGraph};
use nullherz_backends::{ThreadedBackend, AudioBackend};
use control_plane::{TimestampedCommand};
use ipc_layer::{RingBuffer, MpscRingBuffer};
use std::sync::Arc;
use std::thread;

fn main() {
    let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(1024));
    let (garbage_prod, _garbage_cons) = RingBuffer::new(1024).split();
    let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();

    let graph = ProcessorGraph::new();
    let engine = AudioEngine::new(cmd_buffer.clone(), None, None, None, garbage_prod, None, None, None, tel_prod, Box::new(graph));

    let mut backend = ThreadedBackend::new();
    backend.start(engine).unwrap();

    for i in 0..10 {
        println!("Swapping graph iteration {}", i);
        let new_graph = Box::new(ProcessorGraph::new());
        // In this prototype, we'd normally send a command to swap.
        // For simplicity, we just sleep and exit.
        thread::sleep(std::time::Duration::from_millis(50));
    }

    backend.stop();
}
