pub mod builder;
pub mod command_dispatcher;
pub mod graph_manager;
pub mod input_handler;
pub mod metrics;
pub mod processing_kernel;
pub mod resource_recycler;
pub mod telemetry_finalizer;

use std::sync::Arc;
use ipc_layer::Producer;
use nullherz_traits::{TimestampedCommand, ProcessingKernel, MidiConsumer, TopologyMutationConsumer, CommandBundleConsumer};
use crate::processors::{AudioProcessor, TaskPool};
use crate::rt_logging::RtLogger;
use self::metrics::EngineMetrics;
use self::graph_manager::GraphManager;
use self::processing_kernel::StandardKernel;
use self::input_handler::EngineInputHandler;
use self::resource_recycler::ResourceRecycler;
use self::telemetry_finalizer::TelemetryFinalizer;
use nullherz_dna::SampleRegistry;

pub struct EngineHost {
    command_producer: Box<dyn nullherz_traits::CommandProducer>,
}

impl nullherz_traits::Host for EngineHost {
    fn push_command(&self, timestamp_samples: u64, command: nullherz_traits::Command) {
        let _ = self.command_producer.push_command(TimestampedCommand {
            timestamp_samples,
            command,
        });
    }

    fn request_registration(&self, capture_node_idx: u32, sample_id: u64) {
        let _ = self.command_producer.push_command(TimestampedCommand {
            timestamp_samples: 0, // ASAP
            command: nullherz_traits::Command::RegisterCapture { capture_node_idx, sample_id },
        });
    }
}

// SAFETY: AudioEngine is Send and Sync because all of its members are either
// Send/Sync or are atomics that allow safe cross-thread access.
unsafe impl Send for AudioEngine {}
unsafe impl Sync for AudioEngine {}

pub struct AudioEngine {
    command_consumer: Box<dyn nullherz_traits::CommandConsumer>,
    command_producer: Box<dyn nullherz_traits::CommandProducer>,
    midi_consumer: Option<Box<dyn MidiConsumer>>,
    bundle_consumer: Option<Box<dyn CommandBundleConsumer>>,
    topology_consumer: Option<Box<dyn TopologyMutationConsumer>>,

    telemetry_producer: Box<dyn nullherz_traits::TelemetryProducer>,
    sample_counter: u64,
    xrun_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    pending_command: Option<TimestampedCommand>,

    pub metrics: EngineMetrics,
    pub health_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub graph_manager: GraphManager,
    pub resource_recycler: ResourceRecycler,
    pub sample_registry: Arc<SampleRegistry>,
    pub kernel: Box<dyn ProcessingKernel>,
    pub host: Option<EngineHost>,
    pub pool: Option<Box<dyn nullherz_traits::ParallelExecutor>>,
    pub transport: nullherz_traits::Transport,
    pub target_sample_rate: f32,
    pub logger: Arc<RtLogger>,
}

impl AudioEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command_consumer: Box<dyn nullherz_traits::CommandConsumer>,
        command_producer: Box<dyn nullherz_traits::CommandProducer>,
        midi_consumer: Option<Box<dyn MidiConsumer>>,
        bundle_consumer: Option<Box<dyn CommandBundleConsumer>>,
        topology_consumer: Option<Box<dyn TopologyMutationConsumer>>,
        garbage_producer: Producer<Box<dyn AudioProcessor>>,
        overflow_garbage_producer: Option<Producer<Box<dyn AudioProcessor>>>,
        bundle_garbage_producer: Option<Producer<Vec<nullherz_traits::Command>>>,
        bundle_overflow_producer: Option<Producer<Vec<nullherz_traits::Command>>>,
        telemetry_producer: Box<dyn nullherz_traits::TelemetryProducer>,
        initial_graph: Box<dyn AudioProcessor>,
        logger: Arc<RtLogger>,
    ) -> Self {
        Self {
            command_consumer,
            command_producer: dyn_clone::clone_box(&*command_producer),
            midi_consumer,
            bundle_consumer,
            topology_consumer,
            graph_manager: GraphManager::new(initial_graph, garbage_producer, overflow_garbage_producer, logger.clone()),
            resource_recycler: ResourceRecycler::new(bundle_garbage_producer, bundle_overflow_producer),
            sample_registry: Arc::new(SampleRegistry::new()),
            kernel: Box::new(StandardKernel),
            telemetry_producer,
            sample_counter: 0,
            xrun_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            pending_command: None,
            metrics: EngineMetrics::new(),
            health_signal: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            host: Some(EngineHost { command_producer }),
            pool: Some(Box::new(TaskPool::new(4))),
            transport: nullherz_traits::Transport {
                bpm: 120.0,
                beat_position: 0.0,
                is_playing: false,
                sample_rate: 44100.0,
            },
            target_sample_rate: 44100.0,
            logger,
        }
    }

    pub fn xrun_counter(&self) -> std::sync::Arc<std::sync::atomic::AtomicU32> {
        self.xrun_count.clone()
    }

    pub fn set_config(&mut self, config: nullherz_traits::AudioConfig) {
        self.target_sample_rate = config.sample_rate;
        self.transport.sample_rate = config.sample_rate;
        let graph = self.graph_manager.get_active_graph();
        graph.setup(config);
    }

    pub fn set_pending_graph(&self, graph: Box<dyn AudioProcessor>) {
        self.graph_manager.set_pending_graph(graph);
    }

    pub fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _num_samples: usize) {
        self.process(inputs, outputs);
    }

    pub fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        let start_cycles = crate::get_cycles();
        let num_samples = outputs.get(0).map(|o| o.len()).unwrap_or(0);
        if num_samples == 0 { return; }

        let host_ref = self.host.as_ref().map(|h| h as &dyn nullherz_traits::Host);
        // SAFETY: We are on the real-time thread.
        let graph = unsafe { self.graph_manager.swap_if_pending(&self.metrics, &self.health_signal) };

        EngineInputHandler::handle_async_inputs(
            graph,
            &mut self.transport,
            &mut self.bundle_consumer,
            &mut self.topology_consumer,
            &mut self.midi_consumer,
            &mut self.resource_recycler,
            &self.sample_registry,
            &self.metrics,
            &self.health_signal,
        );

        self.kernel.execute(
            graph,
            &mut self.transport,
            host_ref,
            &mut self.pool,
            &mut self.command_consumer,
            &mut self.pending_command,
            self.sample_counter,
            inputs,
            outputs,
            num_samples
        );

        TelemetryFinalizer::finalize_block_telemetry(
            graph,
            &self.metrics,
            &mut self.telemetry_producer,
            &self.xrun_count,
            &mut self.sample_counter,
            start_cycles,
            num_samples
        );
    }
}
