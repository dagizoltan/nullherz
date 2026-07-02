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
use nullherz_traits::{TimestampedCommand, ProcessingKernel, MidiConsumer, TopologyMutationConsumer, CommandBundleConsumer, telemetry::Telemetry};
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
        use nullherz_traits::{Command, ResourceCommand};
        let _ = self.command_producer.push_command(TimestampedCommand {
            timestamp_samples: 0, // ASAP
            command: Command::Resource(ResourceCommand::RegisterCapture { capture_node_idx, sample_id }),
        });
    }
}

// SAFETY: AudioEngine is Send and Sync because all of its members are either
// Send/Sync or are atomics that allow safe cross-thread access.
unsafe impl<K: ProcessingKernel> Send for AudioEngine<K> {}
unsafe impl<K: ProcessingKernel> Sync for AudioEngine<K> {}

/// Encapsulates all IPC resources required by the `AudioEngine`.
/// This includes command streams, MIDI inputs, telemetry producers, and resource recycling channels.
pub struct EngineResources {
    pub command_consumer: Box<dyn nullherz_traits::CommandConsumer>,
    pub command_producer: Box<dyn nullherz_traits::CommandProducer>,
    pub midi_consumer: Option<Box<dyn MidiConsumer>>,
    pub bundle_consumer: Option<Box<dyn CommandBundleConsumer>>,
    pub topology_consumer: Option<Box<dyn TopologyMutationConsumer>>,
    pub garbage_producer: Producer<Box<dyn AudioProcessor>>,
    pub overflow_garbage_producer: Option<Producer<Box<dyn AudioProcessor>>>,
    pub bundle_garbage_producer: Option<Producer<Vec<nullherz_traits::Command>>>,
    pub bundle_overflow_producer: Option<Producer<Vec<nullherz_traits::Command>>>,
    pub telemetry_producer: Box<dyn nullherz_traits::TelemetryProducer>,
}

#[derive(Clone)]
pub struct TelemetryLogEntry {
    pub telemetry: Telemetry,
    pub timestamp_cycles: u64,
}

pub struct AudioEngine<K: ProcessingKernel = StandardKernel> {
    command_consumer: Box<dyn nullherz_traits::CommandConsumer>,
    #[allow(dead_code)]
    command_producer: Box<dyn nullherz_traits::CommandProducer>,
    midi_consumer: Option<Box<dyn MidiConsumer>>,
    bundle_consumer: Option<Box<dyn CommandBundleConsumer>>,
    topology_consumer: Option<Box<dyn TopologyMutationConsumer>>,

    telemetry_producer: Box<dyn nullherz_traits::TelemetryProducer>,
    telemetry_log_producer: Option<ipc_layer::Producer<TelemetryLogEntry>>,
    xrun_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    pending_command: Option<TimestampedCommand>,

    pub metrics: EngineMetrics,
    pub health_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub graph_manager: GraphManager,
    pub resource_recycler: ResourceRecycler,
    pub sample_registry: Arc<SampleRegistry>,
    pub kernel: K,
    pub host: Option<EngineHost>,
    pub pool: Option<Box<dyn nullherz_traits::ParallelExecutor>>,
    pub transport: nullherz_traits::Transport,
    pub target_sample_rate: f32,
    pub logger: Arc<RtLogger>,

    // Pre-allocated FFT resources for RT-safe spectrum analysis
    fft_plan: audio_dsp::SimdFft,
    fft_re: audio_dsp::AlignedBuffer,
    fft_im: audio_dsp::AlignedBuffer,
}

impl<K: ProcessingKernel> nullherz_traits::RenderingEngine for AudioEngine<K> {
    fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize) {
        self.process_block(inputs, outputs, num_samples);
    }

    fn set_config(&mut self, config: nullherz_traits::AudioConfig) {
        self.set_config(config);
    }

    fn target_sample_rate(&self) -> f32 {
        self.target_sample_rate
    }

    fn pull_all_snapshots(&self, target: &mut Vec<(u64, Arc<Vec<f32>>)>) {
        // SAFETY: We are accessing the graph from the orchestration plane (non-RT).
        // The RT thread may be processing, so this access must be read-only safe.
        // SnapshotProvider::pull_all_snapshots technically takes &mut self but it's
        // designed to be lock-free and RT-safe.
        let graph = unsafe { &mut *self.graph_manager.get_active_graph_ptr() };
        graph.pull_all_snapshots(target);
    }

    fn list_children(&self) -> Vec<&dyn AudioProcessor> {
        // SAFETY: Caller must ensure this does not race with RT processing.
        let graph = unsafe { self.graph_manager.get_active_graph_mut() };
        graph.list_children()
    }
}

impl<K: ProcessingKernel> nullherz_traits::RenderingController for AudioEngine<K> {
    fn set_pending_graph(&self, graph: Box<dyn AudioProcessor>) {
        self.set_pending_graph(graph);
    }
}

impl<K: ProcessingKernel> AudioEngine<K> {
    pub fn new(
        resources: EngineResources,
        initial_graph: Box<dyn AudioProcessor>,
        logger: Arc<RtLogger>,
        kernel: K,
    ) -> Self {
        let command_producer = dyn_clone::clone_box(&*resources.command_producer);
        Self {
            command_producer: dyn_clone::clone_box(&*command_producer),
            command_consumer: resources.command_consumer,
            midi_consumer: resources.midi_consumer,
            bundle_consumer: resources.bundle_consumer,
            topology_consumer: resources.topology_consumer,
            graph_manager: GraphManager::new(
                initial_graph,
                resources.garbage_producer,
                resources.overflow_garbage_producer,
                logger.clone()
            ),
            resource_recycler: ResourceRecycler::new(
                resources.bundle_garbage_producer,
                resources.bundle_overflow_producer
            ),
            sample_registry: Arc::new(SampleRegistry::new()),
            kernel,
            telemetry_producer: resources.telemetry_producer,
            telemetry_log_producer: None,
            xrun_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            pending_command: None,
            metrics: EngineMetrics::new(),
            health_signal: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            host: Some(EngineHost { command_producer }),
            pool: Some(Box::new(TaskPool::new(nullherz_traits::DEFAULT_WORKER_COUNT))),
            transport: nullherz_traits::Transport {
                bpm: 120.0,
                beat_position: 0.0,
                is_playing: false,
                sample_rate: 44100.0,
                absolute_samples: 0,
            },
            target_sample_rate: 44100.0,
            logger,
            fft_plan: audio_dsp::SimdFft::new(1024),
            fft_re: audio_dsp::AlignedBuffer::new(1024),
            fft_im: audio_dsp::AlignedBuffer::new(1024),
        }
    }

    pub fn with_flight_recorder(mut self, producer: ipc_layer::Producer<TelemetryLogEntry>) -> Self {
        self.telemetry_log_producer = Some(producer);
        self
    }

    pub fn xrun_counter(&self) -> std::sync::Arc<std::sync::atomic::AtomicU32> {
        self.xrun_count.clone()
    }

    pub fn set_config(&mut self, config: nullherz_traits::AudioConfig) {
        self.target_sample_rate = config.sample_rate;
        self.transport.sample_rate = config.sample_rate;
        // SAFETY: We have &mut self here.
        let graph = unsafe { self.graph_manager.get_active_graph_mut() };
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
        let num_samples = outputs.first().map(|o| o.len()).unwrap_or(0);
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

        let block_start_samples = self.transport.absolute_samples;

        self.kernel.execute(
            graph,
            &mut self.transport,
            host_ref,
            &mut self.pool,
            &mut self.command_consumer,
            &mut self.pending_command,
            block_start_samples,
            inputs,
            outputs,
            num_samples
        );

        let telemetry = TelemetryFinalizer::finalize_block_telemetry(
            graph,
            &self.metrics,
            outputs,
            &mut self.telemetry_producer,
            &self.xrun_count,
            self.transport.absolute_samples,
            start_cycles,
            num_samples,
            &self.fft_plan,
            &mut self.fft_re,
            &mut self.fft_im,
        );

        // Black-Box Flight Recorder (RT-Safe SPSC push)
        if let Some(ref mut log_prod) = self.telemetry_log_producer {
            let _ = log_prod.push(TelemetryLogEntry {
                telemetry,
                timestamp_cycles: start_cycles,
            });
        }
    }
}
