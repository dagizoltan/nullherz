use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::Instant;
use ipc_layer::{Producer, Consumer};
use control_plane::TimestampedCommand;
use crate::processors::AudioProcessor;
use crate::telemetry::Telemetry;

pub struct AudioEngine {
    command_consumer: Consumer<TimestampedCommand>,
    active_graph: AtomicPtr<Box<dyn AudioProcessor>>,
    pending_graph: AtomicPtr<Box<dyn AudioProcessor>>,
    garbage_producer: Producer<Box<dyn AudioProcessor>>,
    telemetry_producer: Producer<Telemetry>,
    sample_counter: u64,
    pending_command: Option<TimestampedCommand>,
}

impl AudioEngine {
    pub fn new(
        command_consumer: Consumer<TimestampedCommand>,
        garbage_producer: Producer<Box<dyn AudioProcessor>>,
        telemetry_producer: Producer<Telemetry>,
        mut initial_graph: Box<dyn AudioProcessor>,
    ) -> Self {
        initial_graph.set_garbage_producer(garbage_producer.clone());
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
    pub fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize) {
        let start_time = Instant::now();

        // 1. RT-Safe Graph Swap
        let pending = self.pending_graph.swap(std::ptr::null_mut(), Ordering::Acquire);
        if !pending.is_null() {
            let old = self.active_graph.swap(pending, Ordering::AcqRel);
            if !old.is_null() {
                let old_graph = unsafe { *Box::from_raw(old) };
                // Send old graph to garbage producer instead of dropping here
                if let Err(leaked) = self.garbage_producer.push(old_graph) {
                    // If queue is full, we must leak to stay RT-safe (rare)
                    let _ = Box::into_raw(Box::new(leaked));
                }
            }
        }

        let graph_ptr = self.active_graph.load(Ordering::Acquire);
        let graph = unsafe { &mut **graph_ptr };

        let mut node_times = [0u64; 64];
        let mut peak_levels = [0.0f32; 64];

        // 2. Block Chunking (Process in chunks of 128 to match AudioBlock capacity)
        let mut processed = 0;
        while processed < num_samples {
            let chunk_size = (num_samples - processed).min(128);
            self.process_chunk(graph, inputs, outputs, processed, chunk_size);
            processed += chunk_size;
        }

        self.sample_counter += num_samples as u64;
        graph.collect_telemetry(&mut node_times, &mut peak_levels);

        let _ = self.telemetry_producer.push(Telemetry {
            process_time_ns: start_time.elapsed().as_nanos() as u64,
            sample_counter: self.sample_counter,
            xrun_count: 0,
            node_times_cycles: node_times,
            peak_levels,
        });
    }

    fn process_chunk(&mut self, graph: &mut dyn AudioProcessor, inputs: &[&[f32]], outputs: &mut [&mut [f32]], chunk_offset: usize, chunk_len: usize) {
        let chunk_start_sample = self.sample_counter + chunk_offset as u64;
        let chunk_end_sample = chunk_start_sample + chunk_len as u64;
        let mut current_in_chunk = 0;

        while current_in_chunk < chunk_len {
            // Take pending command or pop new one
            let cmd = if let Some(pending) = self.pending_command.take() {
                Some(pending)
            } else {
                self.command_consumer.pop()
            };

            if let Some(cmd) = cmd {
                if cmd.timestamp_samples < chunk_end_sample {
                    // Command happens within this chunk
                    let cmd_offset_in_chunk = if cmd.timestamp_samples > chunk_start_sample {
                        (cmd.timestamp_samples - chunk_start_sample) as usize
                    } else {
                        0
                    };

                    if cmd_offset_in_chunk > current_in_chunk {
                        let sub_len = cmd_offset_in_chunk - current_in_chunk;
                        self.process_sub_block(graph, inputs, outputs, chunk_offset + current_in_chunk, sub_len);
                        current_in_chunk += sub_len;
                    }
                    graph.apply_command(&cmd.command);
                } else {
                    // Command is for the future, put back in pending
                    self.pending_command = Some(cmd);
                    let remaining = chunk_len - current_in_chunk;
                    self.process_sub_block(graph, inputs, outputs, chunk_offset + current_in_chunk, remaining);
                    current_in_chunk = chunk_len;
                }
            } else {
                // No more commands for this chunk
                let remaining = chunk_len - current_in_chunk;
                self.process_sub_block(graph, inputs, outputs, chunk_offset + current_in_chunk, remaining);
                current_in_chunk = chunk_len;
            }
        }
    }
    fn process_sub_block(&mut self, graph: &mut dyn AudioProcessor, inputs: &[&[f32]], outputs: &mut [&mut [f32]], offset: usize, len: usize) {
        if len == 0 { return; }
        let mut sub_inputs_ptr = [ &[][..]; crate::MAX_CHANNELS ];
        let num_inputs = inputs.len().min(crate::MAX_CHANNELS);
        for i in 0..num_inputs {
            let input_len = inputs[i].len();
            let end = (offset + len).min(input_len);
            let actual_offset = offset.min(input_len);
            sub_inputs_ptr[i] = &inputs[i][actual_offset..end];
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
