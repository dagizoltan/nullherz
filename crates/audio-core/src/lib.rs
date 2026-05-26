use control_plane::TimestampedCommand;
use ipc_layer::{Consumer, Producer, AudioBlock, ShmRingBuffer};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::thread;
use std::time::{Duration, Instant};

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

pub struct SidecarProcessor {
    command_producer_ptr: *const ShmRingBuffer<control_plane::Command>,
    input_shm: [*mut ShmRingBuffer<AudioBlock>; MAX_CHANNELS],
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
        Self { command_producer_ptr: command_ptr, input_shm, output_shm, num_channels }
    }
}

impl AudioProcessor for SidecarProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        for i in 0..self.num_channels {
            if i < inputs.len() {
                let mut block = AudioBlock { data: [0.0; 128] };
                let len = inputs[i].len().min(128);
                block.data[..len].copy_from_slice(&inputs[i][..len]);
                unsafe { let _ = (*self.input_shm[i]).push(block); }
            }
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
        unsafe { let _ = (*self.command_producer_ptr).push(*command); }
    }
}

pub struct AudioEngine {
    command_consumer: Consumer<TimestampedCommand>,
    active_graph: AtomicPtr<ProcessorChain>,
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

    pub fn request_swap(&self, new_graph: Box<ProcessorChain>) {
        let new_ptr = Box::into_raw(new_graph);
        let old_pending = self.pending_graph.swap(new_ptr, Ordering::AcqRel);
        if !old_pending.is_null() {
            unsafe { drop(Box::from_raw(old_pending)); }
        }
    }

    pub fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize) {
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
        if !ptr.is_null() { unsafe { drop(Box::from_raw(ptr)); } }
        let pending = self.pending_graph.load(Ordering::Acquire);
        if !pending.is_null() { unsafe { drop(Box::from_raw(pending)); } }
    }
}

pub trait AudioBackend {
    fn start(&mut self, engine: AudioEngine) -> Result<(), String>;
    fn stop(&mut self);
}

pub struct ThreadedBackend {
    handle: Option<thread::JoinHandle<()>>,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl ThreadedBackend {
    pub fn new() -> Self {
        Self { handle: None, running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)) }
    }
}

impl AudioBackend for ThreadedBackend {
    fn start(&mut self, mut engine: AudioEngine) -> Result<(), String> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();

        let handle = thread::spawn(move || {
            let inputs_raw = [[0.0f32; 128]; 2];
            let mut outputs_raw = [[0.0f32; 128]; 2];
            let interval = Duration::from_secs_f64(128.0 / 44100.0);

            while running.load(Ordering::SeqCst) {
                let start = Instant::now();

                let in_refs = [&inputs_raw[0][..], &inputs_raw[1][..]];

                // Safe way to get multiple mut references from the same array of arrays
                let (ch1, ch2) = outputs_raw.split_at_mut(1);
                let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];

                engine.process_block(&in_refs, &mut out_refs, 128);

                let elapsed = start.elapsed();
                if elapsed < interval {
                    thread::sleep(interval - elapsed);
                }
            }
        });

        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
