use audio_core::{AudioEngine, ProcessorGraph};
use nullherz_backends::{ThreadedBackend, AudioBackend};
use nullherz_traits::{TimestampedCommand};
use ipc_layer::{RingBuffer, MpscRingBuffer};
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(1024));
    let (garbage_prod, _garbage_cons) = RingBuffer::new(1024).split();
    let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();

    let graph = ProcessorGraph::new();
    let engine = AudioEngine::new(cmd_buffer.clone(), None, None, None, garbage_prod, None, None, None, tel_prod, Box::new(graph));
    let engine_handle = Arc::new(Mutex::new(Some(engine)));

    let mut backend = ThreadedBackend::new();
    backend.start(engine_handle).unwrap();

    for i in 0..10 {
        println!("Swapping graph iteration {}", i);
        let _new_graph = Box::new(ProcessorGraph::new());
        thread::sleep(std::time::Duration::from_millis(50));
    }

    backend.stop();
}
