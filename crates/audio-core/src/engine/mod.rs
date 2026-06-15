pub mod metrics;
pub mod command_dispatcher;
pub mod graph_manager;
pub mod builder;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use ipc_layer::{Producer, Consumer, RingBuffer};
use nullherz_traits::TimestampedCommand;
use crate::processors::{AudioProcessor, TaskPool, ProcessContext};
use nullherz_traits::telemetry::Telemetry;
use crate::rt_logging::RtLogger;
use self::metrics::EngineMetrics;
use self::command_dispatcher::CommandDispatcher;
use self::graph_manager::GraphManager;

pub struct EngineHost {
    command_producer: Arc<ipc_layer::MpscRingBuffer<TimestampedCommand>>,
}

impl nullherz_traits::Host for EngineHost {
    fn push_command(&self, timestamp_samples: u64, command: nullherz_traits::Command) {
        let _ = self.command_producer.push(TimestampedCommand {
            timestamp_samples,
            command,
        });
    }
}

// SAFETY: AudioEngine is Send and Sync because all of its members are either
// Send/Sync or are atomics that allow safe cross-thread access.
unsafe impl Send for AudioEngine {}
unsafe impl Sync for AudioEngine {}

pub struct AudioEngine {
    command_consumer: Arc<ipc_layer::MpscRingBuffer<TimestampedCommand>>,
    midi_consumer: Option<Consumer<ipc_layer::MidiEvent>>,
    bundle_consumer: Option<Consumer<Vec<nullherz_traits::Command>>>,
    topology_consumer: Option<Consumer<nullherz_traits::TopologyMutation>>,
    graph_manager: GraphManager,
    bundle_garbage_producer: Option<Producer<Vec<nullherz_traits::Command>>>,
    bundle_overflow_producer: Option<Producer<Vec<nullherz_traits::Command>>>,
    telemetry_producer: Producer<Telemetry>,
    sample_counter: u64,
    xrun_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    pending_command: Option<TimestampedCommand>,
    pub metrics: EngineMetrics,
    pub health_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub host: Option<EngineHost>,
    pub pool: Option<Box<dyn nullherz_traits::ParallelExecutor>>,
    pub transport: nullherz_traits::Transport,
    pub target_sample_rate: f32,
    pub logger: Arc<RtLogger>,
}

impl AudioEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command_consumer: Arc<ipc_layer::MpscRingBuffer<TimestampedCommand>>,
        midi_consumer: Option<Consumer<ipc_layer::MidiEvent>>,
        bundle_consumer: Option<Consumer<Vec<nullherz_traits::Command>>>,
        topology_consumer: Option<Consumer<nullherz_traits::TopologyMutation>>,
        garbage_producer: Producer<Box<dyn AudioProcessor>>,
        overflow_garbage_producer: Option<Producer<Box<dyn AudioProcessor>>>,
        bundle_garbage_producer: Option<Producer<Vec<nullherz_traits::Command>>>,
        bundle_overflow_producer: Option<Producer<Vec<nullherz_traits::Command>>>,
        telemetry_producer: Producer<Telemetry>,
        initial_graph: Box<dyn AudioProcessor>,
    ) -> Self {
        Self {
            command_consumer: command_consumer.clone(),
            midi_consumer,
            bundle_consumer,
            topology_consumer,
            graph_manager: GraphManager::new(initial_graph, garbage_producer, overflow_garbage_producer),
            bundle_garbage_producer,
            bundle_overflow_producer,
            telemetry_producer,
            sample_counter: 0,
            xrun_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            pending_command: None,
            metrics: EngineMetrics::new(),
            health_signal: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            host: Some(EngineHost { command_producer: command_consumer }),
            pool: Some(Box::new(TaskPool::new(4))),
            transport: nullherz_traits::Transport {
                bpm: 120.0,
                beat_position: 0.0,
                is_playing: false,
                sample_rate: 44100.0,
            },
            target_sample_rate: 44100.0,
            logger: Arc::new(RtLogger::new(256)),
        }
    }

    pub fn xrun_counter(&self) -> std::sync::Arc<std::sync::atomic::AtomicU32> {
        self.xrun_count.clone()
    }
    pub fn set_config(&mut self, config: nullherz_traits::AudioConfig) {
        if (config.sample_rate - self.target_sample_rate).abs() > 0.1 {
            self.logger.log(crate::rt_logging::RtLogLevel::Error, "Hardware rate mismatch", self.sample_counter);
        }
        self.transport.sample_rate = config.sample_rate;
        let graph = self.graph_manager.get_active_graph();
        graph.setup(config);
    }

    pub fn set_pending_graph(&self, graph: Box<dyn AudioProcessor>) {
        self.graph_manager.set_pending_graph(graph);
    }

    pub fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize) {
        crate::setup_rt_thread(90, None);
        let start_cycles = crate::get_cycles();

        self.metrics.calibrate(self.sample_counter, num_samples);

        let graph = unsafe { self.graph_manager.swap_if_pending(&self.metrics, &self.health_signal) };
        let host_ref = self.host.as_ref().map(|h| h as &dyn nullherz_traits::Host);

        Self::handle_async_inputs_static(
            graph,
            &mut self.transport,
            &mut self.bundle_consumer,
            &mut self.topology_consumer,
            &mut self.midi_consumer,
            &mut self.bundle_garbage_producer,
            &mut self.bundle_overflow_producer,
            &self.metrics,
            &self.health_signal,
        );

        Self::execute_processing_kernel_static(
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

        Self::finalize_block_telemetry_static(
            graph,
            &self.metrics,
            &mut self.telemetry_producer,
            &self.xrun_count,
            &mut self.sample_counter,
            start_cycles,
            num_samples
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_async_inputs_static(
        graph: &mut dyn AudioProcessor,
        transport: &mut nullherz_traits::Transport,
        bundle_consumer: &mut Option<Consumer<Vec<nullherz_traits::Command>>>,
        topology_consumer: &mut Option<Consumer<nullherz_traits::TopologyMutation>>,
        midi_consumer: &mut Option<Consumer<ipc_layer::MidiEvent>>,
        bundle_garbage_producer: &mut Option<Producer<Vec<nullherz_traits::Command>>>,
        bundle_overflow_producer: &mut Option<Producer<Vec<nullherz_traits::Command>>>,
        metrics: &EngineMetrics,
        health_signal: &Arc<std::sync::atomic::AtomicBool>,
    ) {
        if let Some(cons) = bundle_consumer {
            while let Some(bundle) = cons.pop() {
                for cmd in &bundle {
                    CommandDispatcher::handle_single_command(transport, graph, cmd);
                }
                Self::recycle_bundle_static(
                    bundle,
                    bundle_garbage_producer,
                    bundle_overflow_producer,
                    metrics,
                    health_signal,
                );
            }
        }

        if let Some(cons) = topology_consumer {
            let mut topo_processed = 0;
            while let Some(topo_mut) = cons.pop() {
                graph.apply_topology_mutation(topo_mut);
                topo_processed += 1;
                if topo_processed >= 16 { break; }
            }
        }

        if let Some(cons) = midi_consumer {
            while let Some(event) = cons.pop() { graph.apply_midi(event); }
        }
    }

    fn recycle_bundle_static(
        bundle: Vec<nullherz_traits::Command>,
        garbage_producer: &mut Option<Producer<Vec<nullherz_traits::Command>>>,
        overflow_producer: &mut Option<Producer<Vec<nullherz_traits::Command>>>,
        metrics: &EngineMetrics,
        health_signal: &Arc<std::sync::atomic::AtomicBool>,
    ) {
        if let Some(prod) = garbage_producer {
            if let Err(b) = prod.push(bundle) {
                if let Some(overflow) = overflow_producer {
                    if let Err(leak) = overflow.push(b) {
                        metrics.report_resource_leak(health_signal);
                        std::mem::forget(leak);
                    }
                } else {
                    metrics.report_resource_leak(health_signal);
                    std::mem::forget(b);
                }
            }
        } else if let Some(overflow) = overflow_producer {
            if let Err(b) = overflow.push(bundle) {
                metrics.report_resource_leak(health_signal);
                std::mem::forget(b);
            }
        } else {
            metrics.report_resource_leak(health_signal);
            std::mem::forget(bundle);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_processing_kernel_static(
        graph: &mut dyn AudioProcessor,
        transport: &mut nullherz_traits::Transport,
        host: Option<&dyn nullherz_traits::Host>,
        pool: &mut Option<Box<dyn nullherz_traits::ParallelExecutor>>,
        command_consumer: &mut Arc<ipc_layer::MpscRingBuffer<TimestampedCommand>>,
        pending_command: &mut Option<TimestampedCommand>,
        sample_counter: u64,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        num_samples: usize,
    ) {
        let block_start_sample = sample_counter;
        let block_end_sample = block_start_sample + num_samples as u64;
        let mut commands_processed = 0;
        const MAX_COMMANDS_PER_BLOCK: usize = 256;

        let mut sub_block_iter = nullherz_traits::SubBlockIterator::new(num_samples, ipc_layer::MAX_BLOCK_SIZE);

        while sub_block_iter.current_offset < num_samples {
            let cmd = if let Some(pending) = pending_command.take() { Some(pending) } else {
                if commands_processed < MAX_COMMANDS_PER_BLOCK { command_consumer.pop() } else { None }
            };

            if let Some(cmd) = cmd {
                if cmd.timestamp_samples < block_end_sample {
                    commands_processed += 1;
                    let cmd_offset = if cmd.timestamp_samples > block_start_sample { (cmd.timestamp_samples - block_start_sample) as usize } else { sub_block_iter.current_offset };

                    while let Some(sb) = sub_block_iter.next_chunk_up_to(cmd_offset) {
                        Self::process_sub_block_and_advance_transport(graph, transport, host, pool, inputs, outputs, sb);
                    }

                    CommandDispatcher::handle_single_command(transport, graph, &cmd.command);
                } else {
                    *pending_command = Some(cmd);
                    while let Some(sb) = sub_block_iter.next_chunk() {
                        Self::process_sub_block_and_advance_transport(graph, transport, host, pool, inputs, outputs, sb);
                    }
                }
            } else {
                while let Some(sb) = sub_block_iter.next_chunk() {
                    Self::process_sub_block_and_advance_transport(graph, transport, host, pool, inputs, outputs, sb);
                }
            }
        }
    }

    fn finalize_block_telemetry_static(
        graph: &mut dyn AudioProcessor,
        metrics: &EngineMetrics,
        telemetry_producer: &mut Producer<Telemetry>,
        xrun_count_atomic: &Arc<std::sync::atomic::AtomicU32>,
        sample_counter: &mut u64,
        start_cycles: u64,
        num_samples: usize,
    ) {
        let mut node_times = [0u64; 64];
        let mut peak_levels = [0.0f32; 64];
        let mut node_times_cycles = [0u64; 64];

        graph.collect_telemetry(&mut node_times_cycles, &mut peak_levels);

        let ns_per_cycle = f64::from_bits(metrics.ns_per_cycle.load(Ordering::Relaxed));
        nullherz_traits::telemetry::TelemetryProcessor::collect_node_times(
            unsafe { std::mem::transmute(&node_times_cycles) },
            ns_per_cycle,
            &mut node_times
        );

        let elapsed_cycles = crate::get_cycles().wrapping_sub(start_cycles);
        let current_ns = (elapsed_cycles as f64 * ns_per_cycle) as u64;
        let peak = metrics.update_peak(current_ns, *sample_counter, num_samples);

        let block_end_sample = *sample_counter + num_samples as u64;
        *sample_counter = block_end_sample;

        let _ = telemetry_producer.push(Telemetry {
            process_time_ns: current_ns, peak_process_time_ns: peak, sample_counter: *sample_counter,
            xrun_count: xrun_count_atomic.load(Ordering::Relaxed), resource_leaks: metrics.resource_leaks.load(Ordering::Relaxed),
            node_times_ns: node_times, peak_levels,
        });
    }

    fn process_sub_block_and_advance_transport(
        graph: &mut dyn AudioProcessor,
        transport: &mut nullherz_traits::Transport,
        host: Option<&dyn nullherz_traits::Host>,
        pool: &mut Option<Box<dyn nullherz_traits::ParallelExecutor>>,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        sb: nullherz_traits::SubBlock,
    ) {
        Self::process_sub_block_static(graph, transport, host, pool, inputs, outputs, sb.offset, sb.len, sb.is_last);
        if transport.is_playing {
            let beats = (sb.len as f64 / transport.sample_rate as f64) * (transport.bpm as f64 / 60.0);
            transport.beat_position += beats;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn process_sub_block_static(
        graph: &mut dyn AudioProcessor,
        transport: &nullherz_traits::Transport,
        host: Option<&dyn nullherz_traits::Host>,
        pool: &mut Option<Box<dyn nullherz_traits::ParallelExecutor>>,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        offset: usize,
        len: usize,
        is_last_sub_block: bool,
    ) {
        if len == 0 { return; }
        let mut context = ProcessContext { transport: Some(transport), host, sub_block_offset: offset, is_last_sub_block };
        let mut sub_inputs_ptr = [ &[][..]; crate::MAX_CHANNELS ];
        let num_inputs = inputs.len().min(crate::MAX_CHANNELS);
        let empty_input = &[][..];
        for (i, sub_input) in sub_inputs_ptr.iter_mut().enumerate().take(num_inputs) {
            let input = inputs.get(i).copied().unwrap_or(empty_input);
            let end = (offset + len).min(input.len());
            let act = offset.min(input.len());
            *sub_input = &input[act..end];
        }
        let mut sub_outputs_reconstructed: [&mut [f32]; crate::MAX_CHANNELS] = std::array::from_fn(|_| &mut [][..]);
        let num_outputs = outputs.len().min(crate::MAX_CHANNELS);
        for (i, out) in outputs.iter_mut().take(num_outputs).enumerate() {
            let end = (offset + len).min(out.len());
            let act = offset.min(out.len());
            if end > act { sub_outputs_reconstructed[i] = &mut out[act..end]; }
        }

        graph.process_parallel(&sub_inputs_ptr[..num_inputs], &mut sub_outputs_reconstructed[..num_outputs], &mut context, pool.as_deref_mut());
    }


}

// GraphManager handles its own drop.
