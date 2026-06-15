use std::sync::Arc;
use ipc_layer::{RingBuffer, MpscRingBuffer, Consumer};
use control_plane::TimestampedCommand;
use nullherz_traits::{AudioProcessor, telemetry::Telemetry};
use crate::engine::AudioEngine;
use crate::processors::ProcessorGraph;

pub struct EngineHandle {
    pub command_producer: Arc<MpscRingBuffer<TimestampedCommand>>,
    pub telemetry_consumer: Consumer<Telemetry>,
    pub health_signal: Arc<std::sync::atomic::AtomicBool>,
}

pub struct EngineBuilder {
    command_buffer_size: usize,
    telemetry_buffer_size: usize,
    initial_graph: Option<Box<dyn AudioProcessor>>,
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self {
            command_buffer_size: 1024,
            telemetry_buffer_size: 1024,
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

        let (garbage_prod, _garbage_cons) = RingBuffer::new(1024).split();
        let (tel_prod, tel_cons) = RingBuffer::new(self.telemetry_buffer_size).split();

        let initial_graph = self.initial_graph.unwrap_or_else(|| Box::new(ProcessorGraph::new()));

        let engine = AudioEngine::new(
            cmd_cons,
            None, // midi
            None, // bundle
            None, // topology
            garbage_prod,
            None, // overflow garbage
            None, // bundle garbage
            None, // bundle overflow
            tel_prod,
            initial_graph
        );

        let handle = EngineHandle {
            command_producer: cmd_buffer,
            telemetry_consumer: tel_cons,
            health_signal: engine.health_signal.clone(),
        };

        (engine, handle)
    }
}
