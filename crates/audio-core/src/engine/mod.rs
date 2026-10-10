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
use nullherz_traits::SampleRegistry;

/// Default worker-pool size when neither the resource config nor
/// `NULLHERZ_WORKERS` specifies one. Leaves the RT thread its own core
/// (`available_parallelism - 1`); the per-stage cost gate means these are an
/// upper bound, not a guarantee of use. Falls back to the historical default
/// if the core count can't be read. Never call on the RT path.
fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1))
        .unwrap_or(nullherz_traits::DEFAULT_WORKER_COUNT)
}

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
    pub worker_count: Option<usize>,
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
    pub sample_registry: Arc<dyn SampleRegistry>,
    pub kernel: K,
    pub host: Option<EngineHost>,
    pub pool: Option<Box<dyn nullherz_traits::ParallelExecutor>>,
    pub transport: nullherz_traits::Transport,
    /// The rate the device is asked for, and the rate the transport follows.
    ///
    /// An atomic rather than an `f32` so `set_config` can take `&self`. The
    /// alternative was for the control plane to obtain `&mut` to the engine,
    /// which it can only do by casting a shared `Arc` — see
    /// `TECHNICAL_DEBT_AND_STUBS.md` §1.1.
    pub target_sample_rate: std::sync::atomic::AtomicU32,
    pub logger: Arc<RtLogger>,

    // Pre-allocated FFT resources for RT-safe spectrum analysis
    fft_plan: audio_dsp::SimdFft,
    fft_re: audio_dsp::AlignedBuffer,
    fft_im: audio_dsp::AlignedBuffer,
    spectral_cache: telemetry_finalizer::SpectralTelemetryCache,
}

impl<K: ProcessingKernel> nullherz_traits::RenderingEngine for AudioEngine<K> {
    fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize) {
        self.process_block(inputs, outputs, num_samples);
    }

    fn set_config(&self, config: nullherz_traits::AudioConfig) {
        self.set_config(config);
    }

    fn target_sample_rate(&self) -> f32 {
        f32::from_bits(self.target_sample_rate.load(std::sync::atomic::Ordering::Relaxed))
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
        sample_registry: Arc<dyn SampleRegistry>,
        logger: Arc<RtLogger>,
        kernel: K,
    ) -> Self {
        // Resolve the SIMD dispatch probe HERE, on the setup thread. `level()`
        // runs CPUID on first use; the audio callback is not where to discover
        // that. Same reasoning as the resampler's sinc table.
        audio_dsp::dispatch::prewarm();
        println!("AudioEngine: DSP SIMD path = {}", audio_dsp::dispatch::level_name());

        let command_producer = dyn_clone::clone_box(&*resources.command_producer);
        // Worker-count resolution: explicit resource config, then the
        // NULLHERZ_WORKERS env override (0 = no pool, pure serial execution
        // on the RT thread), then a core-count-aware default. This is an
        // UPPER BOUND on parallelism, not a mandate: the executor's per-stage
        // cost gate decides whether any given stage actually dispatches, so
        // spare workers just park (see the 2026-07-21 worker experiment).
        // Read here, at construction, on the setup thread — never on the RT path.
        let worker_count = resources.worker_count
            .or_else(|| std::env::var("NULLHERZ_WORKERS").ok().and_then(|v| v.parse::<usize>().ok()))
            .unwrap_or_else(default_worker_count)
            .min(64);
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
            sample_registry,
            kernel,
            telemetry_producer: resources.telemetry_producer,
            telemetry_log_producer: None,
            xrun_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            pending_command: None,
            metrics: EngineMetrics::new(),
            health_signal: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            host: Some(EngineHost { command_producer }),
            pool: if worker_count == 0 { None } else { Some(Box::new(TaskPool::new(worker_count))) },
            transport: nullherz_traits::Transport {
                bpm: 120.0,
                beat_position: 0.0,
                is_playing: false,
                sample_rate: nullherz_traits::DEFAULT_SAMPLE_RATE,
                absolute_samples: 0,
                system_time_ns: 0,
                device_time_ns: 0,
            },
            target_sample_rate: std::sync::atomic::AtomicU32::new(
                nullherz_traits::DEFAULT_SAMPLE_RATE.to_bits(),
            ),
            logger,
            fft_plan: audio_dsp::SimdFft::new(1024),
            fft_re: audio_dsp::AlignedBuffer::new(1024),
            fft_im: audio_dsp::AlignedBuffer::new(1024),
            spectral_cache: telemetry_finalizer::SpectralTelemetryCache::default(),
        }
    }

    /// Attach a black-box recorder that captures `Telemetry` per block.
    ///
    /// **OFF by default, and you must drain it.** It was previously attached
    /// unconditionally by `EngineBuilder::build` while nothing anywhere read the
    /// consumer, which made it worse than useless in two ways:
    ///
    ///  * `Telemetry` is 8,912 bytes and `Copy`, so every block paid a full copy
    ///    into the ring — on the audio thread, forever, for nobody.
    ///  * The ring is 128 deep and an SPSC `Producer` REFUSES when full rather
    ///    than overwriting. Undrained it fills in 128 blocks (0.74 s at 256/44.1k)
    ///    and every push after that fails. So it held the FIRST three quarters of
    ///    a second of a session and discarded the rest — the opposite of what a
    ///    flight recorder is for, which is the moments before a fault.
    ///
    /// If you attach one, drain `EngineHandle::telemetry_log_consumer` from the
    /// orchestration thread faster than blocks arrive, or you are recording the
    /// wrong end of the session.
    pub fn with_flight_recorder(mut self, producer: ipc_layer::Producer<TelemetryLogEntry>) -> Self {
        self.telemetry_log_producer = Some(producer);
        self
    }

    pub fn xrun_counter(&self) -> std::sync::Arc<std::sync::atomic::AtomicU32> {
        self.xrun_count.clone()
    }

    /// Re-configure the engine. Takes `&self`.
    ///
    /// `&self`, not `&mut self`, and that is the point: the engine lives in an
    /// `Arc<dyn RenderingEngine>`, so a caller wanting `&mut` could only get it
    /// by casting the shared `Arc` — which is UB, and which every caller of
    /// this method was doing. `Arc::get_mut` is not an escape: a second clone
    /// lives in `EngineHandle::controller` for the life of the session, so it
    /// never succeeds. See `TECHNICAL_DEBT_AND_STUBS.md` §1.1.
    ///
    /// Nothing here needed `&mut` in the first place:
    ///
    ///  - `target_sample_rate` is an atomic.
    ///  - `graph.setup()` goes through `get_active_graph_mut`, which was
    ///    already a `&self` method.
    ///  - `transport.sample_rate` is the only field that genuinely belongs to
    ///    the RT thread, so it is no longer written from here at all: `process`
    ///    picks the rate up from the atomic at the top of each block.
    ///
    /// PRECONDITION, unchanged: the caller must not be racing the audio thread.
    /// `graph.setup()` resizes node buffers, and no `&self` signature makes that
    /// safe to do under a live renderer — `ConfigureAudioEngine` stops the
    /// device first, and an `OfflineRenderer` never starts one.
    pub fn set_config(&self, config: nullherz_traits::AudioConfig) {
        self.target_sample_rate
            .store(config.sample_rate.to_bits(), std::sync::atomic::Ordering::Relaxed);
        // SAFETY: same contract as every other caller of this accessor — no
        // other thread is touching the graph. See the precondition above.
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
        nullherz_traits::assert_rt_safe!();
        let _fp_guard = ipc_layer::FpControlGuard::new();
        let start_cycles = crate::get_cycles();
        let num_samples = outputs.first().map(|o| o.len()).unwrap_or(0);
        if num_samples == 0 { return; }

        // Adopt a configured rate, if one landed since the last block.
        //
        // `set_config` takes `&self` and cannot write RT-owned state, so the
        // transport follows the atomic here instead — one relaxed load and a
        // compare, on the thread that owns `transport`.
        let configured =
            f32::from_bits(self.target_sample_rate.load(std::sync::atomic::Ordering::Relaxed));
        if self.transport.sample_rate != configured {
            self.transport.sample_rate = configured;
        }

        let host_ref = self.host.as_ref().map(|h| h as &dyn nullherz_traits::Host);
        // SAFETY: We are on the real-time thread.
        let graph = unsafe { self.graph_manager.swap_if_pending(&self.metrics, &self.health_signal) };

        // Phase order matters: structural mutations must be INSTALLED into the
        // node array (commit_graph) before command bundles run — a PlayNode
        // broadcast in the same block as bootstrap otherwise fires into
        // DummyProcessors and is lost forever (fire-once semantics).
        EngineInputHandler::handle_topology_inputs(graph, &mut self.topology_consumer);
        if let Some(graph_concrete) = graph.as_any_mut().downcast_mut::<crate::processors::graph::ProcessorGraph>() {
            graph_concrete.commit_graph();
        }
        EngineInputHandler::handle_async_inputs(
            graph,
            &mut self.transport,
            &mut self.bundle_consumer,
            &mut self.topology_consumer,
            &mut self.midi_consumer,
            &mut self.resource_recycler,
            self.sample_registry.as_ref(),
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

        TelemetryFinalizer::finalize_block_telemetry(
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
            &self.transport,
            &mut self.spectral_cache,
            &mut self.telemetry_log_producer,
        );
    }
}
