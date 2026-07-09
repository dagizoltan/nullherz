use std::sync::Arc;
use ipc_layer::{RingBuffer, MpscRingBuffer};
use nullherz_traits::{AudioProcessor};
use audio_core::{AudioEngine, ProcessorGraph};

fn main() {
    let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();
    let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
    let cmd_buffer = Arc::new(MpscRingBuffer::new(1024));

    let graph = ProcessorGraph::new();

    let resources = audio_core::engine::EngineResources {
        command_consumer: Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
        command_producer: Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
        midi_consumer: None,
        bundle_consumer: None,
        topology_consumer: None,
        garbage_producer: garbage_prod,
        overflow_garbage_producer: None,
        bundle_garbage_producer: None,
        bundle_overflow_producer: None,
        telemetry_producer: Box::new(tel_prod),
        worker_count: None,
    };

    let engine = AudioEngine::new(
        resources,
        Box::new(graph),
        Arc::new(nullherz_dna::SampleRegistry::new()), Arc::new(audio_core::rt_logging::RtLogger::new(256)),
        audio_core::engine::processing_kernel::StandardKernel::default()
    );

    println!("Engine created. Swapping graphs...");

    let new_graph = ProcessorGraph::new();
    engine.set_pending_graph(Box::new(new_graph));

    println!("Graph swap scheduled.");
}
