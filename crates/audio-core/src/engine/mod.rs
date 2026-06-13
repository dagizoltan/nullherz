pub mod metrics;
pub mod command_dispatcher;

use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, Ordering};
use ipc_layer::{Producer, Consumer};
use control_plane::TimestampedCommand;
use crate::processors::{AudioProcessor, TaskPool, ProcessContext};
use nullherz_traits::telemetry::Telemetry;
use crate::rt_logging::RtLogger;
use self::metrics::EngineMetrics;
use self::command_dispatcher::CommandDispatcher;

// SAFETY: AudioEngine is Send and Sync because all of its members are either
// Send/Sync or are atomics that allow safe cross-thread access.
unsafe impl Send for AudioEngine {}
unsafe impl Sync for AudioEngine {}

pub struct AudioEngine {
    command_consumer: Arc<ipc_layer::MpscRingBuffer<TimestampedCommand>>,
    midi_consumer: Option<Consumer<ipc_layer::MidiEvent>>,
    bundle_consumer: Option<Consumer<Vec<control_plane::Command>>>,
    topology_consumer: Option<Consumer<nullherz_traits::TopologyMutation>>,
    active_graph: AtomicPtr<Box<dyn AudioProcessor>>,
    pending_graph: AtomicPtr<Box<dyn AudioProcessor>>,
    garbage_producer: Producer<Box<dyn AudioProcessor>>,
    overflow_garbage_producer: Option<Producer<Box<dyn AudioProcessor>>>,
    bundle_garbage_producer: Option<Producer<Vec<control_plane::Command>>>,
    bundle_overflow_producer: Option<Producer<Vec<control_plane::Command>>>,
    telemetry_producer: Producer<Telemetry>,
    sample_counter: u64,
    xrun_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    pending_command: Option<TimestampedCommand>,
    pub metrics: EngineMetrics,
    pub health_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
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
        bundle_consumer: Option<Consumer<Vec<control_plane::Command>>>,
        topology_consumer: Option<Consumer<nullherz_traits::TopologyMutation>>,
        garbage_producer: Producer<Box<dyn AudioProcessor>>,
        overflow_garbage_producer: Option<Producer<Box<dyn AudioProcessor>>>,
        bundle_garbage_producer: Option<Producer<Vec<control_plane::Command>>>,
        bundle_overflow_producer: Option<Producer<Vec<control_plane::Command>>>,
        telemetry_producer: Producer<Telemetry>,
        initial_graph: Box<dyn AudioProcessor>,
    ) -> Self {
        Self {
            command_consumer,
            midi_consumer,
            bundle_consumer,
            topology_consumer,
            active_graph: AtomicPtr::new(Box::into_raw(Box::new(initial_graph))),
            pending_graph: AtomicPtr::new(std::ptr::null_mut()),
            garbage_producer,
            overflow_garbage_producer,
            bundle_garbage_producer,
            bundle_overflow_producer,
            telemetry_producer,
            sample_counter: 0,
            xrun_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            pending_command: None,
            metrics: EngineMetrics::new(),
            health_signal: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
        let graph_ptr = self.active_graph.load(Ordering::Acquire);
        let graph = unsafe { &mut **graph_ptr };
        graph.setup(config);
    }

    pub fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize) {
        crate::setup_rt_thread(90, None);
        let start_cycles = crate::get_cycles();

        self.metrics.calibrate(self.sample_counter, num_samples);

        let pending = self.pending_graph.swap(std::ptr::null_mut(), Ordering::Acquire);
        if !pending.is_null() {
            let old = self.active_graph.swap(pending, Ordering::AcqRel);
            if !old.is_null() {
                let old_graph = unsafe { Box::from_raw(old) };
                if let Err(leaked) = self.garbage_producer.push(*old_graph) {
                    if let Some(ref mut overflow) = self.overflow_garbage_producer {
                        if let Err(leaked) = overflow.push(leaked) {
                            self.metrics.report_resource_leak(&self.health_signal);
                            let _ = Box::into_raw(Box::new(leaked));
                        }
                    } else {
                        self.metrics.report_resource_leak(&self.health_signal);
                        let _ = Box::into_raw(Box::new(leaked));
                    }
                }
            }
        }
        let block_start_sample = self.sample_counter;
        let block_end_sample = block_start_sample + num_samples as u64;
        let mut current_sample_in_block = 0;
        let graph_ptr = self.active_graph.load(Ordering::Acquire);
        let graph = unsafe { &mut **graph_ptr };

        let mut commands_processed = 0;
        const MAX_COMMANDS_PER_BLOCK: usize = 256;

        if self.bundle_consumer.is_some() {
            while let Some(bundle) = self.bundle_consumer.as_mut().unwrap().pop() {
                for cmd in &bundle {
                    CommandDispatcher::handle_single_command(&mut self.transport, graph, cmd);
                }
                if let Some(ref mut prod) = self.bundle_garbage_producer {
                    if let Err(b) = prod.push(bundle) {
                        if let Some(ref mut overflow) = self.bundle_overflow_producer {
                            if let Err(b) = overflow.push(b) {
                                self.metrics.report_resource_leak(&self.health_signal);
                                std::mem::forget(b);
                            }
                        } else {
                            self.metrics.report_resource_leak(&self.health_signal);
                            std::mem::forget(b);
                        }
                    }
                } else if let Some(ref mut overflow) = self.bundle_overflow_producer {
                    if let Err(b) = overflow.push(bundle) {
                        self.metrics.report_resource_leak(&self.health_signal);
                        std::mem::forget(b);
                    }
                } else {
                    self.metrics.report_resource_leak(&self.health_signal);
                    std::mem::forget(bundle);
                }
            }
        }

        if let Some(ref mut cons) = self.topology_consumer {
            let mut topo_processed = 0;
            while let Some(topo_mut) = cons.pop() {
                graph.apply_topology_mutation(topo_mut);
                topo_processed += 1;
                if topo_processed >= 16 { break; }
            }
        }

        if let Some(ref mut cons) = self.midi_consumer {
            while let Some(event) = cons.pop() { graph.apply_midi(event); }
        }

        let mut node_times = [0u64; 64];
        let mut peak_levels = [0.0f32; 64];
        let mut node_times_cycles = [0u64; 64];

        while current_sample_in_block < num_samples {
            let cmd = if let Some(pending) = self.pending_command.take() { Some(pending) } else {
                if commands_processed < MAX_COMMANDS_PER_BLOCK { self.command_consumer.pop() } else { None }
            };
            if let Some(cmd) = cmd {
                if cmd.timestamp_samples < block_end_sample {
                    commands_processed += 1;
                    let cmd_offset = if cmd.timestamp_samples > block_start_sample { (cmd.timestamp_samples - block_start_sample) as usize } else { current_sample_in_block };
                    if cmd_offset > current_sample_in_block {
                        let mut remaining_to_cmd = cmd_offset - current_sample_in_block;
                        while remaining_to_cmd > 0 {
                            let chunk = remaining_to_cmd.min(ipc_layer::MAX_BLOCK_SIZE);
                            let is_last = (current_sample_in_block + chunk) == num_samples;
                            self.process_sub_block(graph, inputs, outputs, current_sample_in_block, chunk, is_last);
                            current_sample_in_block += chunk;
                            remaining_to_cmd -= chunk;
                        }
                    }
                    CommandDispatcher::handle_single_command(&mut self.transport, graph, &cmd.command);
                } else {
                    self.pending_command = Some(cmd);
                    let mut remaining = num_samples - current_sample_in_block;
                    while remaining > 0 {
                        let chunk = remaining.min(ipc_layer::MAX_BLOCK_SIZE);
                        let is_last = (current_sample_in_block + chunk) == num_samples;
                        self.process_sub_block(graph, inputs, outputs, current_sample_in_block, chunk, is_last);
                        current_sample_in_block += chunk;
                        remaining -= chunk;
                    }
                }
            } else {
                let mut remaining = num_samples - current_sample_in_block;
                while remaining > 0 {
                    let chunk = remaining.min(ipc_layer::MAX_BLOCK_SIZE);
                    let is_last = (current_sample_in_block + chunk) == num_samples;
                    self.process_sub_block(graph, inputs, outputs, current_sample_in_block, chunk, is_last);
                    current_sample_in_block += chunk;
                    remaining -= chunk;
                }
            }
        }
        self.sample_counter = block_end_sample;
        graph.collect_telemetry(&mut node_times_cycles, &mut peak_levels);

        let ns_per_cycle = f64::from_bits(self.metrics.ns_per_cycle.load(Ordering::Relaxed));
        nullherz_traits::telemetry::TelemetryProcessor::collect_node_times(
            unsafe { std::mem::transmute(&node_times_cycles) },
            ns_per_cycle,
            &mut node_times
        );

        let elapsed_cycles = crate::get_cycles().wrapping_sub(start_cycles);
        let current_ns = (elapsed_cycles as f64 * ns_per_cycle) as u64;
        let peak = self.metrics.update_peak(current_ns, self.sample_counter, num_samples);

        let _ = self.telemetry_producer.push(Telemetry {
            process_time_ns: current_ns, peak_process_time_ns: peak, sample_counter: self.sample_counter,
            xrun_count: self.xrun_count.load(Ordering::Relaxed), resource_leaks: self.metrics.resource_leaks.load(Ordering::Relaxed),
            node_times_ns: node_times, peak_levels,
        });
    }


    fn process_sub_block(&mut self, graph: &mut dyn AudioProcessor, inputs: &[&[f32]], outputs: &mut [&mut [f32]], offset: usize, len: usize, is_last_sub_block: bool) {
        if len == 0 { return; }
        let mut context = ProcessContext { transport: Some(&self.transport), sub_block_offset: offset, is_last_sub_block };
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

        graph.process_parallel(&sub_inputs_ptr[..num_inputs], &mut sub_outputs_reconstructed[..num_outputs], &mut context, self.pool.as_deref_mut());

        if self.transport.is_playing {
            let beats = (len as f64 / self.transport.sample_rate as f64) * (self.transport.bpm as f64 / 60.0);
            self.transport.beat_position += beats;
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let ptr = self.active_graph.load(Ordering::Acquire);
        if !ptr.is_null() { unsafe { drop(Box::from_raw(ptr)); } }
        let pending = self.pending_graph.load(Ordering::Acquire);
        if !pending.is_null() { unsafe { drop(Box::from_raw(pending)); } }
    }
}
