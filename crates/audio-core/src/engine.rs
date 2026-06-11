use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, Ordering};
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
use std::time::Instant;
use ipc_layer::{Producer, Consumer};
use control_plane::TimestampedCommand;
use crate::processors::{AudioProcessor, TaskPool, ProcessContext};
use crate::telemetry::Telemetry;
use crate::rt_logging::{RtLogger, RtLogLevel};

// SAFETY: AudioEngine is Send and Sync because all of its members are either
// Send/Sync or are atomics that allow safe cross-thread access.
// Specifically:
// 1. AtomicPtr<Box<dyn AudioProcessor>>: AudioProcessor is Send, and we use
//    proper Atomic ordering (Acquire/Release) to ensure visibility of the
//    underlying processor when swapping or dereferencing.
// 2. The engine is intended to be moved to a backend thread for the RT loop.
unsafe impl Send for AudioEngine {}
unsafe impl Sync for AudioEngine {}

pub struct AudioEngine {
    command_consumer: Arc<ipc_layer::MpscRingBuffer<TimestampedCommand>>,
    midi_consumer: Option<Consumer<ipc_layer::MidiEvent>>,
    bundle_consumer: Option<Consumer<Vec<control_plane::Command>>>,
    topology_consumer: Option<Consumer<crate::processors::TopologyMutation>>,
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
    ns_per_cycle: std::sync::Arc<std::sync::atomic::AtomicU64>,
    peak_ns: std::sync::atomic::AtomicU64,
    resource_leaks: std::sync::atomic::AtomicU64,
    pub health_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub pool: Option<TaskPool>,
    pub transport: crate::Transport,
    pub target_sample_rate: f32,
    pub logger: Arc<RtLogger>,
}

impl AudioEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command_consumer: Arc<ipc_layer::MpscRingBuffer<TimestampedCommand>>,
        midi_consumer: Option<Consumer<ipc_layer::MidiEvent>>,
        bundle_consumer: Option<Consumer<Vec<control_plane::Command>>>,
        topology_consumer: Option<Consumer<crate::processors::TopologyMutation>>,
        garbage_producer: Producer<Box<dyn AudioProcessor>>,
        overflow_garbage_producer: Option<Producer<Box<dyn AudioProcessor>>>,
        bundle_garbage_producer: Option<Producer<Vec<control_plane::Command>>>,
        bundle_overflow_producer: Option<Producer<Vec<control_plane::Command>>>,
        telemetry_producer: Producer<Telemetry>,
        initial_graph: Box<dyn AudioProcessor>,
    ) -> Self {
        let ns_per_cycle = std::sync::Arc::new(std::sync::atomic::AtomicU64::new((1.0f64).to_bits()));
        let ns_per_cycle_clone = ns_per_cycle.clone();
        std::thread::spawn(move || {
            let calibrated = Self::calibrate_cycles();
            ns_per_cycle_clone.store(calibrated.to_bits(), Ordering::Relaxed);
        });

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
            ns_per_cycle,
            peak_ns: std::sync::atomic::AtomicU64::new(0),
            resource_leaks: std::sync::atomic::AtomicU64::new(0),
            health_signal: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            pool: Some(TaskPool::new(4)),
            transport: crate::Transport {
                bpm: 120.0,
                beat_position: 0.0,
                is_playing: false,
                sample_rate: 44100.0,
            },
            target_sample_rate: 44100.0,
            logger: Arc::new(RtLogger::new(256)),
        }
    }

    fn calibrate_cycles() -> f64 {
        let mut ratios = Vec::with_capacity(7);

        for _ in 0..7 {
            let start = std::time::Instant::now();
            let start_c = Self::get_cycles();
            // Busy wait for ~10ms for more accurate calibration than sleep
            while start.elapsed() < std::time::Duration::from_millis(10) {
                std::hint::spin_loop();
            }
            let elapsed = start.elapsed().as_nanos() as f64;
            let elapsed_c = (Self::get_cycles().wrapping_sub(start_c)) as f64;
            if elapsed_c > 0.0 {
                ratios.push(elapsed / elapsed_c);
            }
        }
        if ratios.is_empty() { return 1.0; }
        ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // Return median
        ratios[ratios.len() / 2]
    }

    #[inline(always)]
    fn get_cycles() -> u64 {
        #[cfg(target_arch = "x86_64")]
        { unsafe { std::arch::x86_64::_rdtsc() } }
        #[cfg(target_arch = "aarch64")]
        {
            unsafe {
                let val: u64;
                std::arch::asm!("mrs {}, cntvct_el0", out(reg) val, options(nomem, nostack));
                val
            }
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            // Fallback for non-x86/ARM platforms using a monotonic clock if possible.
            // Since this is for telemetry/calibration, resolution matters.
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            { 0 }
            #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
            {
                static BASELINE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
                let start = BASELINE.get_or_init(std::time::Instant::now);
                start.elapsed().as_nanos() as u64
            }
        }
    }
    pub fn xrun_counter(&self) -> std::sync::Arc<std::sync::atomic::AtomicU32> {
        self.xrun_count.clone()
    }
    pub fn set_config(&mut self, config: crate::AudioConfig) {
        if (config.sample_rate - self.target_sample_rate).abs() > 0.1 {
            self.logger.log(RtLogLevel::Error, "Hardware rate mismatch", self.sample_counter);
        }
        self.transport.sample_rate = config.sample_rate;
        let graph_ptr = self.active_graph.load(Ordering::Acquire);
        let graph = unsafe { &mut **graph_ptr };
        graph.setup(config);
    }

    pub fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize) {
        // Ensure Denormals-Are-Zero and Flush-to-Zero are set for this thread
        // (Backends might have reset them or it might be a new thread)
        crate::setup_rt_thread(90, None);

        let start_cycles = Self::get_cycles();

        let pending = self.pending_graph.swap(std::ptr::null_mut(), Ordering::Acquire);
        if !pending.is_null() {
            let old = self.active_graph.swap(pending, Ordering::AcqRel);
            if !old.is_null() {
                let old_graph = unsafe { Box::from_raw(old) };
                if let Err(leaked) = self.garbage_producer.push(*old_graph) {
                    if let Some(ref mut overflow) = self.overflow_garbage_producer {
                        if let Err(leaked) = overflow.push(leaked) {
                            self.resource_leaks.fetch_add(1, Ordering::Relaxed);
                            self.health_signal.store(true, Ordering::Relaxed);
                            let _ = Box::into_raw(Box::new(leaked));
                        }
                    } else {
                        self.resource_leaks.fetch_add(1, Ordering::Relaxed);
                        self.health_signal.store(true, Ordering::Relaxed);
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

        // Process atomic command bundles first (immediate application)
        if let Some(ref mut cons) = self.bundle_consumer {
            while let Some(bundle) = cons.pop() {
                for cmd in &bundle {
                    match cmd {
                        control_plane::Command::Play => {
                            self.transport.is_playing = true;
                            graph.apply_command(cmd);
                        }
                        control_plane::Command::Stop => {
                            self.transport.is_playing = false;
                            graph.apply_command(cmd);
                        }
                        control_plane::Command::Bundle { count, data } => {
                            // Expand small bundle data if present
                            for i in 0..(*count as usize).min(4) {
                                let node_id = data[i * 3];
                                let param_id = data[i * 3 + 1] as u32;
                                let value = f32::from_bits(data[i * 3 + 2] as u32);
                                graph.apply_command(&control_plane::Command::SetParam {
                                    target_id: node_id,
                                    param_id,
                                    value,
                                    ramp_duration_samples: 0,
                                });
                            }
                        }
                        _ => {
                            graph.apply_command(cmd);
                        }
                    }
                }
                // RT-safe deallocation: offload the vector to a garbage consumer
                if let Some(ref mut prod) = self.bundle_garbage_producer {
                    if let Err(b) = prod.push(bundle) {
                        if let Some(ref mut overflow) = self.bundle_overflow_producer {
                            if let Err(b) = overflow.push(b) {
                                self.resource_leaks.fetch_add(1, Ordering::Relaxed);
                                self.health_signal.store(true, Ordering::Relaxed);
                                std::mem::forget(b);
                            }
                        } else {
                            self.resource_leaks.fetch_add(1, Ordering::Relaxed);
                            self.health_signal.store(true, Ordering::Relaxed);
                            std::mem::forget(b);
                        }
                    }
                } else if let Some(ref mut overflow) = self.bundle_overflow_producer {
                    if let Err(b) = overflow.push(bundle) {
                        self.resource_leaks.fetch_add(1, Ordering::Relaxed);
                        self.health_signal.store(true, Ordering::Relaxed);
                        std::mem::forget(b);
                    }
                } else {
                    // If no producer exists, we must leak the Vec to avoid dropping it here.
                    self.resource_leaks.fetch_add(1, Ordering::Relaxed);
                    self.health_signal.store(true, Ordering::Relaxed);
                    std::mem::forget(bundle);
                }
            }
        }

        if let Some(ref mut cons) = self.topology_consumer {
            let mut topo_processed = 0;
            while let Some(topo_mut) = cons.pop() {
                graph.apply_topology_mutation(topo_mut);
                topo_processed += 1;
                if topo_processed >= 16 { break; } // Limit topology mutations per block
            }
        }

        if let Some(ref mut cons) = self.midi_consumer {
            while let Some(event) = cons.pop() {
                graph.apply_midi(event);
            }
        }

        let mut node_times = [0u64; 64];
        let mut peak_levels = [0.0f32; 64];

        let mut node_times_cycles = [0u64; 64];

        while current_sample_in_block < num_samples {
            let cmd = if let Some(pending) = self.pending_command.take() { Some(pending) } else {
                if commands_processed < MAX_COMMANDS_PER_BLOCK {
                    self.command_consumer.pop()
                } else {
                    None
                }
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
                    match cmd.command {
                        control_plane::Command::Play => self.transport.is_playing = true,
                        control_plane::Command::Stop => self.transport.is_playing = false,
                        _ => {}
                    }
                    graph.apply_command(&cmd.command);
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

        let ns_per_cycle = f64::from_bits(self.ns_per_cycle.load(Ordering::Relaxed));

        for (i, node_time) in node_times.iter_mut().enumerate() {
            *node_time = (node_times_cycles.get(i).copied().unwrap_or(0) as f64 * ns_per_cycle) as u64;
        }

        let elapsed_cycles = Self::get_cycles().wrapping_sub(start_cycles);

        let current_ns = (elapsed_cycles as f64 * ns_per_cycle) as u64;
        let mut peak = self.peak_ns.load(Ordering::Relaxed);
        if current_ns > peak {
            let _ = self.peak_ns.compare_exchange(peak, current_ns, Ordering::Relaxed, Ordering::Relaxed);
            peak = current_ns;
        }

        // Reset peak every ~1000 blocks to track moving jitter
        if self.sample_counter.is_multiple_of(num_samples as u64 * 1024) {
            self.peak_ns.store(current_ns, Ordering::Relaxed);
        }

        let _ = self.telemetry_producer.push(Telemetry {
            process_time_ns: current_ns,
            peak_process_time_ns: peak,
            sample_counter: self.sample_counter,
            xrun_count: self.xrun_count.load(Ordering::Relaxed),
            resource_leaks: self.resource_leaks.load(Ordering::Relaxed),
            node_times_ns: node_times,
            peak_levels,
        });
    }
    fn process_sub_block(&mut self, graph: &mut dyn AudioProcessor, inputs: &[&[f32]], outputs: &mut [&mut [f32]], offset: usize, len: usize, is_last_sub_block: bool) {
        if len == 0 { return; }
        let mut context = ProcessContext {
            pool: self.pool.as_mut(),
            transport: Some(&self.transport),
            sub_block_offset: offset,
            is_last_sub_block,
        };
        let mut sub_inputs_ptr = [ &[][..]; crate::MAX_CHANNELS ];
        let num_inputs = inputs.len().min(crate::MAX_CHANNELS);
        let empty_input = &[][..];
        for (i, sub_input) in sub_inputs_ptr.iter_mut().enumerate().take(num_inputs) {
            let input = inputs.get(i).unwrap_or(&empty_input);
            let input_len = input.len();
            let end = (offset + len).min(input_len);
            let actual_offset = offset.min(input_len);
            *sub_input = &input[actual_offset..end];
        }

        let mut sub_outputs_reconstructed: [&mut [f32]; crate::MAX_CHANNELS] = std::array::from_fn(|_| &mut [][..]);
        let num_outputs = outputs.len().min(crate::MAX_CHANNELS);
        for (i, out) in outputs.iter_mut().take(num_outputs).enumerate() {
            let output_len = out.len();
            let end = (offset + len).min(output_len);
            let actual_offset = offset.min(output_len);
            if end > actual_offset {
                sub_outputs_reconstructed[i] = &mut out[actual_offset..end];
            }
        }

        graph.process(&sub_inputs_ptr[..num_inputs], &mut sub_outputs_reconstructed[..num_outputs], &mut context);

        if self.transport.is_playing {
            let seconds_per_block = len as f64 / self.transport.sample_rate as f64;
            let beats_per_block = seconds_per_block * (self.transport.bpm as f64 / 60.0);
            self.transport.beat_position += beats_per_block;
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
