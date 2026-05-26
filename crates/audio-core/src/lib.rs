use control_plane::TimestampedCommand;
use ipc_layer::{Consumer, Producer, AudioBlock, ShmRingBuffer};
use std::sync::atomic::{AtomicPtr, Ordering};

pub trait AudioProcessor: Send {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]);
    fn apply_command(&mut self, _command: &control_plane::Command) {}
}

pub struct ProcessorChain {
    processors: Vec<Box<dyn AudioProcessor>>,
}

impl ProcessorChain {
    pub fn new() -> Self {
        Self { processors: Vec::new() }
    }

    pub fn add(&mut self, processor: Box<dyn AudioProcessor>) {
        self.processors.push(processor);
    }
}

impl AudioProcessor for ProcessorChain {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        for processor in &mut self.processors {
            processor.process(inputs, outputs);
        }
    }

    fn apply_command(&mut self, command: &control_plane::Command) {
        for processor in &mut self.processors {
            processor.apply_command(command);
        }
    }
}

pub const MAX_CHANNELS: usize = 16;

/// A processor that represents an external sidecar process.
pub struct SidecarProcessor {
    command_producer_ptr: *const ShmRingBuffer<control_plane::Command>,
    /// Array of input buffers for each channel.
    input_shm: [*mut ShmRingBuffer<AudioBlock>; MAX_CHANNELS],
    /// Array of output buffers for each channel.
    output_shm: [*const ShmRingBuffer<AudioBlock>; MAX_CHANNELS],
    num_channels: usize,
}

unsafe impl Send for SidecarProcessor {}

impl SidecarProcessor {
    pub unsafe fn new(
        command_ptr: *const ShmRingBuffer<control_plane::Command>,
        inputs: &[*mut ShmRingBuffer<AudioBlock>],
        outputs: &[*const ShmRingBuffer<AudioBlock>],
    ) -> Self {
        let mut input_shm = [std::ptr::null_mut(); MAX_CHANNELS];
        let mut output_shm = [std::ptr::null(); MAX_CHANNELS];
        let num_channels = inputs.len().min(MAX_CHANNELS).min(outputs.len());

        for i in 0..num_channels {
            input_shm[i] = inputs[i];
            output_shm[i] = outputs[i];
        }

        Self {
            command_producer_ptr: command_ptr,
            input_shm,
            output_shm,
            num_channels,
        }
    }
}

impl AudioProcessor for SidecarProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        for i in 0..self.num_channels {
            // 1. Push input audio to shared memory
            if i < inputs.len() {
                let mut block = AudioBlock { data: [0.0; 128] };
                let len = inputs[i].len().min(128);
                block.data[..len].copy_from_slice(&inputs[i][..len]);
                unsafe { let _ = (*self.input_shm[i]).push(block); }
            }

            // 2. Try to pop processed audio from shared memory
            if i < outputs.len() {
                unsafe {
                    if let Some(block) = (*self.output_shm[i]).pop() {
                        let len = outputs[i].len().min(128);
                        outputs[i][..len].copy_from_slice(&block.data[..len]);
                    }
                }
            }
        }
    }

    fn apply_command(&mut self, command: &control_plane::Command) {
        unsafe {
            let _ = (*self.command_producer_ptr).push(*command);
        }
    }
}

pub struct AudioEngine {
    command_consumer: Consumer<TimestampedCommand>,
    active_graph: AtomicPtr<ProcessorChain>,
    /// Pending graph update from Control Plane.
    pending_graph: AtomicPtr<ProcessorChain>,

    garbage_producer: Producer<Box<ProcessorChain>>,
    sample_counter: u64,
    pending_command: Option<TimestampedCommand>,
}

impl AudioEngine {
    pub fn new(
        command_consumer: Consumer<TimestampedCommand>,
        garbage_producer: Producer<Box<ProcessorChain>>,
        initial_graph: Box<ProcessorChain>,
    ) -> Self {
        Self {
            command_consumer,
            active_graph: AtomicPtr::new(Box::into_raw(initial_graph)),
            pending_graph: AtomicPtr::new(std::ptr::null_mut()),
            garbage_producer,
            sample_counter: 0,
            pending_command: None,
        }
    }

    /// Requests a graph swap. This is safe to call from Control Plane.
    pub fn request_swap(&self, new_graph: Box<ProcessorChain>) {
        let new_ptr = Box::into_raw(new_graph);
        // We only allow one pending swap at a time for simplicity.
        // If a swap is already pending, we'll have to handle it (e.g. drop the new one or replace it).
        let old_pending = self.pending_graph.swap(new_ptr, Ordering::AcqRel);
        if !old_pending.is_null() {
            unsafe { drop(Box::from_raw(old_pending)); }
        }
    }

    pub fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize) {
        // 0. Check for pending graph swap
        let pending = self.pending_graph.swap(std::ptr::null_mut(), Ordering::Acquire);
        if !pending.is_null() {
            let old = self.active_graph.swap(pending, Ordering::AcqRel);
            if !old.is_null() {
                let old_graph = unsafe { Box::from_raw(old) };
                let _ = self.garbage_producer.push(old_graph);
            }
        }

        let block_start_sample = self.sample_counter;
        let block_end_sample = block_start_sample + num_samples as u64;
        let mut current_sample_in_block = 0;

        let graph_ptr = self.active_graph.load(Ordering::Acquire);
        let graph = unsafe { &mut *graph_ptr };

        while current_sample_in_block < num_samples {
            let cmd = if let Some(pending) = self.pending_command.take() {
                Some(pending)
            } else {
                self.command_consumer.pop()
            };

            if let Some(cmd) = cmd {
                if cmd.timestamp_samples < block_end_sample {
                    let cmd_offset = if cmd.timestamp_samples > block_start_sample {
                        (cmd.timestamp_samples - block_start_sample) as usize
                    } else {
                        0
                    };

                    if cmd_offset > current_sample_in_block {
                        let samples_to_process = cmd_offset - current_sample_in_block;
                        self.process_sub_block(graph, inputs, outputs, current_sample_in_block, samples_to_process);
                        current_sample_in_block += samples_to_process;
                    }

                    graph.apply_command(&cmd.command);
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
    }

    fn process_sub_block(&mut self, graph: &mut ProcessorChain, inputs: &[&[f32]], outputs: &mut [&mut [f32]], offset: usize, len: usize) {
        if len == 0 { return; }
        let mut sub_inputs_ptr = [ &[][..]; MAX_CHANNELS ];
        let num_inputs = inputs.len().min(MAX_CHANNELS);
        for i in 0..num_inputs {
            sub_inputs_ptr[i] = &inputs[i][offset..offset+len];
        }
        let mut sub_outputs_ptrs: [*mut f32; MAX_CHANNELS] = [std::ptr::null_mut(); MAX_CHANNELS];
        let num_outputs = outputs.len().min(MAX_CHANNELS);
        for i in 0..num_outputs {
            sub_outputs_ptrs[i] = outputs[i][offset..offset+len].as_mut_ptr();
        }
        let mut sub_outputs_reconstructed: [&mut [f32]; MAX_CHANNELS] = std::array::from_fn(|i| {
            if i < num_outputs {
                unsafe { std::slice::from_raw_parts_mut(sub_outputs_ptrs[i], len) }
            } else {
                &mut []
            }
        });
        graph.process(&sub_inputs_ptr[..num_inputs], &mut sub_outputs_reconstructed[..num_outputs]);
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let ptr = self.active_graph.load(Ordering::Acquire);
        if !ptr.is_null() {
            unsafe { drop(Box::from_raw(ptr)); }
        }
        let pending = self.pending_graph.load(Ordering::Acquire);
        if !pending.is_null() {
            unsafe { drop(Box::from_raw(pending)); }
        }
    }
}

pub trait AudioBackend {
    fn start(&mut self, engine: AudioEngine) -> Result<(), String>;
    fn stop(&mut self);
}
