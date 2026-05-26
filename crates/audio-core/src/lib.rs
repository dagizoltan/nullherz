use control_plane::TimestampedCommand;
use ipc_layer::Consumer;

/// Base trait for any audio processing unit.
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

pub struct AudioEngine {
    command_consumer: Consumer<TimestampedCommand>,
    main_chain: ProcessorChain,
    sample_counter: u64,
    pending_command: Option<TimestampedCommand>,
}

impl AudioEngine {
    pub fn new(command_consumer: Consumer<TimestampedCommand>) -> Self {
        Self {
            command_consumer,
            main_chain: ProcessorChain::new(),
            sample_counter: 0,
            pending_command: None,
        }
    }

    pub fn add_processor(&mut self, processor: Box<dyn AudioProcessor>) {
        self.main_chain.add(processor);
    }

    pub fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize) {
        let block_start_sample = self.sample_counter;
        let block_end_sample = block_start_sample + num_samples as u64;

        let mut current_sample_in_block = 0;

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
                        self.process_sub_block(inputs, outputs, current_sample_in_block, samples_to_process);
                        current_sample_in_block += samples_to_process;
                    }

                    self.main_chain.apply_command(&cmd.command);
                } else {
                    self.pending_command = Some(cmd);
                    let remaining = num_samples - current_sample_in_block;
                    self.process_sub_block(inputs, outputs, current_sample_in_block, remaining);
                    current_sample_in_block = num_samples;
                }
            } else {
                let remaining = num_samples - current_sample_in_block;
                self.process_sub_block(inputs, outputs, current_sample_in_block, remaining);
                current_sample_in_block = num_samples;
            }
        }

        self.sample_counter = block_end_sample;
    }

    fn process_sub_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], offset: usize, len: usize) {
        if len == 0 { return; }

        let mut sub_inputs_ptr = [ &[][..]; MAX_CHANNELS ];
        let num_inputs = inputs.len().min(MAX_CHANNELS);
        for i in 0..num_inputs {
            sub_inputs_ptr[i] = &inputs[i][offset..offset+len];
        }

        // Use a pointer array to bypass the lack of Copy for &mut [f32].
        let mut sub_outputs_ptrs: [*mut f32; MAX_CHANNELS] = [std::ptr::null_mut(); MAX_CHANNELS];
        let mut sub_outputs_lens: [usize; MAX_CHANNELS] = [0; MAX_CHANNELS];
        let num_outputs = outputs.len().min(MAX_CHANNELS);

        for i in 0..num_outputs {
            let slice = &mut outputs[i][offset..offset+len];
            sub_outputs_ptrs[i] = slice.as_mut_ptr();
            sub_outputs_lens[i] = slice.len();
        }

        // Reconstruct &mut [&mut [f32]] using a temporary stack array of &mut [f32].
        // This is safe because the lifetime of the reconstructed slices is tied to this block.
        let mut sub_outputs_reconstructed: [&mut [f32]; MAX_CHANNELS] = std::array::from_fn(|i| {
            if i < num_outputs {
                unsafe { std::slice::from_raw_parts_mut(sub_outputs_ptrs[i], sub_outputs_lens[i]) }
            } else {
                &mut []
            }
        });

        self.main_chain.process(&sub_inputs_ptr[..num_inputs], &mut sub_outputs_reconstructed[..num_outputs]);
    }
}
