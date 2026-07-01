use std::sync::Arc;
use ipc_layer::{RingBuffer, MpscRingBuffer, Producer, Consumer};
use nullherz_traits::{AudioProcessor, TopologyMutation, MidiEvent, telemetry::Telemetry};
use crate::engine::AudioEngine;
use crate::processors::ProcessorGraph;

pub struct EngineHandle {
    pub command_producer: Box<dyn nullherz_traits::CommandProducer>,
    pub controller: Arc<dyn nullherz_traits::RenderingController>,
    pub midi_producer: Producer<MidiEvent>,
    pub bundle_producer: Producer<Vec<nullherz_traits::Command>>,
    pub topology_producer: Producer<TopologyMutation>,
    pub telemetry_consumer: Consumer<Telemetry>,
    pub telemetry_log_consumer: Option<Consumer<crate::engine::TelemetryLogEntry>>,
    pub garbage_consumer: Option<Consumer<Box<dyn AudioProcessor>>>,
    pub garbage_overflow_consumer: Option<Consumer<Box<dyn AudioProcessor>>>,
    pub bundle_garbage_consumer: Option<Consumer<Vec<nullherz_traits::Command>>>,
    pub bundle_overflow_consumer: Option<Consumer<Vec<nullherz_traits::Command>>>,
    pub health_signal: Arc<std::sync::atomic::AtomicBool>,
}

pub struct EngineBuilder {
    command_buffer_size: usize,
    midi_buffer_size: usize,
    bundle_buffer_size: usize,
    topology_buffer_size: usize,
    telemetry_buffer_size: usize,
    garbage_buffer_size: usize,
    initial_graph: Option<Box<dyn AudioProcessor>>,
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self {
            command_buffer_size: 1024,
            midi_buffer_size: 256,
            bundle_buffer_size: 64,
            topology_buffer_size: 64,
            telemetry_buffer_size: 1024,
            garbage_buffer_size: 1024,
            initial_graph: None,
        }
    }
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_command_buffer_size(mut self, size: usize) -> Self {
        self.command_buffer_size = size;
        self
    }

    pub fn with_initial_graph(mut self, graph: Box<dyn AudioProcessor>) -> Self {
        self.initial_graph = Some(graph);
        self
    }

    pub fn build(self) -> (Arc<AudioEngine>, EngineHandle) {
        let cmd_buffer = Arc::new(MpscRingBuffer::new(self.command_buffer_size));
        let cmd_cons = ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone());
        let cmd_prod = ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone());

        let (midi_prod, midi_cons) = RingBuffer::new(self.midi_buffer_size).split();
        let (bundle_prod, bundle_cons) = RingBuffer::new(self.bundle_buffer_size).split();
        let (topo_prod, topo_cons) = RingBuffer::new(self.topology_buffer_size).split();
        let (tel_prod, tel_cons) = RingBuffer::new(self.telemetry_buffer_size).split();
        let (tel_log_prod, tel_log_cons) = RingBuffer::new(128).split();
        let (garbage_prod, garbage_cons) = RingBuffer::new(self.garbage_buffer_size).split();

        // Optional overflow and deallocation cues
        let (bundle_garbage_prod, bundle_garbage_cons) = RingBuffer::new(32).split();
        let (bundle_overflow_prod, bundle_overflow_cons) = RingBuffer::new(32).split();
        let (garbage_overflow_prod, garbage_overflow_cons) = RingBuffer::new(32).split();

        let initial_graph = self.initial_graph.unwrap_or_else(|| Box::new(ProcessorGraph::new()));

        let resources = crate::engine::EngineResources {
            command_consumer: Box::new(cmd_cons),
            command_producer: Box::new(cmd_prod.clone()),
            midi_consumer: Some(Box::new(midi_cons)),
            bundle_consumer: Some(Box::new(bundle_cons)),
            topology_consumer: Some(Box::new(topo_cons)),
            garbage_producer: garbage_prod,
            overflow_garbage_producer: Some(garbage_overflow_prod),
            bundle_garbage_producer: Some(bundle_garbage_prod),
            bundle_overflow_producer: Some(bundle_overflow_prod),
            telemetry_producer: Box::new(tel_prod),
        };

        let engine = AudioEngine::new(
            resources,
            initial_graph,
            Arc::new(crate::rt_logging::RtLogger::new(256)),
            crate::engine::processing_kernel::StandardKernel::default()
        ).with_flight_recorder(tel_log_prod);

        let engine = Arc::new(engine);

        let handle = EngineHandle {
            command_producer: Box::new(cmd_prod),
            controller: engine.clone(),
            midi_producer: midi_prod,
            bundle_producer: bundle_prod,
            topology_producer: topo_prod,
            telemetry_consumer: tel_cons,
            telemetry_log_consumer: Some(tel_log_cons),
            garbage_consumer: Some(garbage_cons),
            garbage_overflow_consumer: Some(garbage_overflow_cons),
            bundle_garbage_consumer: Some(bundle_garbage_cons),
            bundle_overflow_consumer: Some(bundle_overflow_cons),
            health_signal: engine.health_signal.clone(),
        };

        (engine, handle)
    }
}
