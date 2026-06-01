use crate::traits::AudioProcessor;
use control_plane::TimestampedCommand;
use ipc_layer::{Consumer, Producer};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::Instant;
use serde_big_array::BigArray;

#[repr(C)]
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Telemetry {
    pub process_time_ns: u64,
    pub sample_counter: u64,
    pub xrun_count: u32,
    #[serde(with = "BigArray")]
    pub node_load_ns: [u64; 64],
    #[serde(with = "BigArray")]
    pub node_avg_load_ns: [u64; 64],
    #[serde(with = "BigArray")]
    pub optimization_suggestions: [u8; 64],
    #[serde(with = "BigArray")]
    pub buffer_levels: [f32; 64],
}

/// The central coordinator for the real-time audio thread.
///
/// `AudioEngine` is responsible for:
/// 1. Managing the active and pending processing graphs via atomic swaps.
/// 2. Executing sample-accurate command automation by splitting audio blocks.
/// 3. Reporting real-time telemetry back to the control plane.
/// 4. Disposing of old graphs through a safe garbage queue.
pub const MAX_CHANNELS: usize = 16;

pub struct AudioEngine {
    pub(crate) command_consumer: Consumer<TimestampedCommand>,
    pub(crate) active_graph: AtomicPtr<Box<dyn AudioProcessor>>,
    pub(crate) pending_graph: AtomicPtr<Box<dyn AudioProcessor>>,
    pub(crate) garbage_producer: Producer<Box<Box<dyn AudioProcessor>>>,
    pub(crate) telemetry_producer: Producer<Telemetry>,
    pub(crate) sample_counter: u64,
    pub(crate) pending_command: Option<TimestampedCommand>,
}

impl AudioEngine {
    pub fn new(
        command_consumer: Consumer<TimestampedCommand>,
        garbage_producer: Producer<Box<Box<dyn AudioProcessor>>>,
        telemetry_producer: Producer<Telemetry>,
        initial_graph: Box<dyn AudioProcessor>,
    ) -> Self {
        Self {
            command_consumer,
            active_graph: AtomicPtr::new(Box::into_raw(Box::new(initial_graph))),
            pending_graph: AtomicPtr::new(std::ptr::null_mut()),
            garbage_producer,
            telemetry_producer,
            sample_counter: 0,
            pending_command: None,
        }
    }
    pub fn request_swap(&self, new_graph: Box<dyn AudioProcessor>) {
        let new_ptr = Box::into_raw(Box::new(new_graph));
        let old_pending = self.pending_graph.swap(new_ptr, Ordering::AcqRel);
        if !old_pending.is_null() { unsafe { drop(Box::from_raw(old_pending)); } }
    }
    /// Processes a single block of audio data.
    ///
    /// This method implements sample-accurate command handling by splitting the block into
    /// sub-blocks whenever a command's timestamp falls within the current processing window.
    /// This ensures that parameter changes and topological updates take effect at the
    /// exact sample offset specified by the control plane.
    pub fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize) {
        let start_time = Instant::now();
        let pending = self.pending_graph.swap(std::ptr::null_mut(), Ordering::Acquire);
        if !pending.is_null() {
            let old = self.active_graph.swap(pending, Ordering::AcqRel);
            if !old.is_null() {
                let old_graph = unsafe { Box::from_raw(old) };
                if let Err(leaked) = self.garbage_producer.push(old_graph) {
                    let _ = Box::into_raw(leaked);
                }
            }
        }
        let block_start_sample = self.sample_counter;
        let block_end_sample = block_start_sample + num_samples as u64;
        let mut current_sample_in_block = 0;
        let graph_ptr = self.active_graph.load(Ordering::Acquire);
        if graph_ptr.is_null() { return; }
        let graph = unsafe { &mut **graph_ptr };
        while current_sample_in_block < num_samples {
            let cmd = if let Some(pending) = self.pending_command.take() { Some(pending) } else { self.command_consumer.pop() };
            if let Some(mut cmd) = cmd {
                if cmd.timestamp_samples < block_end_sample {
                    let cmd_offset = if cmd.timestamp_samples > block_start_sample { (cmd.timestamp_samples - block_start_sample) as usize } else { 0 };
                    let samples_before_cmd = cmd_offset.saturating_sub(current_sample_in_block);

                    if samples_before_cmd > 0 {
                        self.process_sub_block(graph, inputs, outputs, current_sample_in_block, samples_before_cmd);
                        current_sample_in_block += samples_before_cmd;
                    }

                    graph.apply_command(&cmd.command);

                    loop {
                        if let Some(next_cmd) = self.command_consumer.pop() {
                            if next_cmd.timestamp_samples <= cmd.timestamp_samples {
                                graph.apply_command(&next_cmd.command);
                                cmd = next_cmd;
                                continue;
                            } else {
                                self.pending_command = Some(next_cmd);
                                break;
                            }
                        }
                        break;
                    }
                } else {
                    self.pending_command = Some(cmd);
                    let remaining = num_samples - current_sample_in_block;
                    self.process_sub_block(graph, inputs, outputs, current_sample_in_block, remaining);
                    current_sample_in_block = num_samples;
                }
            } else {
                let remaining = num_samples - current_sample_in_block;
                self.process_sub_block(graph, inputs, outputs, current_sample_in_block, remaining);
                current_sample_in_block = num_samples;
            }
        }
        self.sample_counter = block_end_sample;
        let mut node_load_ns = [0u64; 64];
        let mut node_avg_load_ns = [0u64; 64];
        let mut suggestions = [0u8; 64];
        let mut buffer_levels = [0.0f32; 64];
        graph.get_telemetry(&mut node_load_ns, &mut node_avg_load_ns, &mut suggestions, &mut buffer_levels);

        let _ = self.telemetry_producer.push(Telemetry {
            process_time_ns: start_time.elapsed().as_nanos() as u64,
            sample_counter: self.sample_counter,
            xrun_count: 0,
            node_load_ns,
            node_avg_load_ns,
            optimization_suggestions: suggestions,
            buffer_levels,
        });
    }
    pub fn last_telemetry(&self) -> Option<Telemetry> {
        None
    }

    fn process_sub_block(&mut self, graph: &mut dyn AudioProcessor, inputs: &[&[f32]], outputs: &mut [&mut [f32]], offset: usize, len: usize) {
        if len == 0 { return; }
        let mut sub_inputs_ptr = [ &[][..]; MAX_CHANNELS ];
        let num_inputs = inputs.len().min(MAX_CHANNELS);
        for i in 0..num_inputs { sub_inputs_ptr[i] = &inputs[i][offset..offset+len]; }
        let mut sub_outputs_ptrs: [*mut f32; MAX_CHANNELS] = [std::ptr::null_mut(); MAX_CHANNELS];
        let num_outputs = outputs.len().min(MAX_CHANNELS);
        for i in 0..num_outputs { sub_outputs_ptrs[i] = outputs[i][offset..offset+len].as_mut_ptr(); }
        let mut sub_outputs_reconstructed: [&mut [f32]; MAX_CHANNELS] = std::array::from_fn(|i| {
            if i < num_outputs { unsafe { std::slice::from_raw_parts_mut(sub_outputs_ptrs[i], len) } } else { &mut [] }
        });
        graph.process(&sub_inputs_ptr[..num_inputs], &mut sub_outputs_reconstructed[..num_outputs]);
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
