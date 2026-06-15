use std::sync::Arc;
use ipc_layer::{RingBuffer, MpscRingBuffer, Producer, Consumer};
use nullherz_traits::TimestampedCommand;
use nullherz_traits::{AudioProcessor, TopologyMutation, MidiEvent, telemetry::Telemetry};
use crate::engine::AudioEngine;
use crate::processors::ProcessorGraph;

pub struct EngineHandle {
    pub command_producer: Arc<MpscRingBuffer<TimestampedCommand>>,
    pub midi_producer: Producer<MidiEvent>,
    pub bundle_producer: Producer<Vec<nullherz_traits::Command>>,
    pub topology_producer: Producer<TopologyMutation>,
    pub telemetry_consumer: Consumer<Telemetry>,
    pub garbage_consumer: Consumer<Box<dyn AudioProcessor>>,
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

    pub fn build(self) -> (AudioEngine, EngineHandle) {
        let cmd_buffer = Arc::new(MpscRingBuffer::new(self.command_buffer_size));
        let cmd_cons = cmd_buffer.clone();

        let (midi_prod, midi_cons) = RingBuffer::new(self.midi_buffer_size).split();
        let (bundle_prod, bundle_cons) = RingBuffer::new(self.bundle_buffer_size).split();
        let (topo_prod, topo_cons) = RingBuffer::new(self.topology_buffer_size).split();
        let (tel_prod, tel_cons) = RingBuffer::new(self.telemetry_buffer_size).split();
        let (garbage_prod, garbage_cons) = RingBuffer::new(self.garbage_buffer_size).split();

        // Optional overflow and deallocation cues
        let (bundle_garbage_prod, _bundle_garbage_cons) = RingBuffer::new(32).split();
        let (bundle_overflow_prod, _bundle_overflow_cons) = RingBuffer::new(32).split();
        let (garbage_overflow_prod, _garbage_overflow_cons) = RingBuffer::new(32).split();

        let initial_graph = self.initial_graph.unwrap_or_else(|| Box::new(ProcessorGraph::new()));

        let engine = AudioEngine::new(
            cmd_cons,
            Some(midi_cons),
            Some(bundle_cons),
            Some(topo_cons),
            garbage_prod,
            Some(garbage_overflow_prod),
            Some(bundle_garbage_prod),
            Some(bundle_overflow_prod),
            tel_prod,
            initial_graph
        );

        let handle = EngineHandle {
            command_producer: cmd_buffer,
            midi_producer: midi_prod,
            bundle_producer: bundle_prod,
            topology_producer: topo_prod,
            telemetry_consumer: tel_cons,
            garbage_consumer: garbage_cons,
            health_signal: engine.health_signal.clone(),
        };

        (engine, handle)
    }
}
