use ipc_layer::{ShmRingBuffer, AudioBlock, ShmSignal, EventFd};
pub use nullherz_traits::{AudioProcessor, ProcessContext};

use ipc_layer::SharedMemory;

/// A sidecar DSP process context.
pub struct SidecarHost {
    shm_cmd: SharedMemory,
    shm_signal: SharedMemory,
    shm_inputs: Vec<SharedMemory>,
    shm_outputs: Vec<SharedMemory>,
    event_fd: Option<EventFd>,
}

impl SidecarHost {
    /// # Safety
    /// All shared memory segment names must exist and be accessible by the current process.
    pub unsafe fn new(cmd_name: &str, sig_name: &str, in_names: &[String], out_names: &[String], efd: i32) -> Self {
        let (cmd_layout, _) = ShmRingBuffer::<nullherz_traits::Command>::layout(64);
        let shm_cmd = SharedMemory::open(cmd_name, cmd_layout.size()).unwrap();

        let shm_signal = SharedMemory::open(sig_name, std::mem::size_of::<ShmSignal>()).unwrap();

        let (audio_layout, _) = ShmRingBuffer::<AudioBlock>::layout(16);
        let mut shm_inputs = Vec::new();
        for name in in_names {
            shm_inputs.push(SharedMemory::open(name, audio_layout.size()).unwrap());
        }

        let mut shm_outputs = Vec::new();
        for name in out_names {
            shm_outputs.push(SharedMemory::open(name, audio_layout.size()).unwrap());
        }

        let event_fd = if efd >= 0 { Some(EventFd::from_raw(efd)) } else { None };

        Self {
            shm_cmd,
            shm_signal,
            shm_inputs,
            shm_outputs,
            event_fd,
        }
    }

    pub fn run(&mut self, processor: impl AudioProcessor) {
        let mut context = SidecarContext::new(
            processor,
            &self.shm_cmd,
            &self.shm_signal,
            &self.shm_inputs,
            &self.shm_outputs,
            self.event_fd.take()
        );

        context.run_loop();
    }
}

pub struct SidecarContext<'a, P: AudioProcessor> {
    processor: P,
    command_buffer: &'a ShmRingBuffer<nullherz_traits::Command>,
    #[allow(dead_code)]
    feedback_buffer: Option<&'a ShmRingBuffer<nullherz_traits::ProcessorMetadata>>,
    input_buffers: Vec<&'a ShmRingBuffer<AudioBlock>>,
    output_buffers: Vec<&'a ShmRingBuffer<AudioBlock>>,
    signal: &'a ShmSignal,
    event_fd: Option<EventFd>,
}

impl<'a, P: AudioProcessor> SidecarContext<'a, P> {
    pub fn new(
        processor: P,
        shm_cmd: &'a SharedMemory,
        shm_signal: &'a SharedMemory,
        shm_inputs: &'a [SharedMemory],
        shm_outputs: &'a [SharedMemory],
        event_fd: Option<EventFd>,
    ) -> Self {
        let command_buffer = unsafe { &*(shm_cmd.ptr() as *const ShmRingBuffer<nullherz_traits::Command>) };
        let signal = unsafe { &*(shm_signal.ptr() as *const ShmSignal) };
        let mut input_buffers = Vec::new();
        for shm in shm_inputs {
            input_buffers.push(unsafe { &*(shm.ptr() as *const ShmRingBuffer<AudioBlock>) });
        }
        let mut output_buffers = Vec::new();
        for shm in shm_outputs {
            output_buffers.push(unsafe { &*(shm.ptr() as *const ShmRingBuffer<AudioBlock>) });
        }

        Self {
            processor,
            command_buffer,
            feedback_buffer: None,
            input_buffers,
            output_buffers,
            signal,
            event_fd,
        }
    }

    pub fn process_once(&mut self) {
        self.signal.pulse_heartbeat();
        while let Some(cmd) = self.command_buffer.pop() {
            self.processor.apply_command(&cmd);
        }

        let mut in_blocks = [AudioBlock { data: [0.0; ipc_layer::MAX_BLOCK_SIZE], len: 0 }; 16];
        let mut out_blocks = [AudioBlock { data: [0.0; ipc_layer::MAX_BLOCK_SIZE], len: 0 }; 16];
        let num_channels = self.input_buffers.len().min(self.output_buffers.len()).min(16);

        let mut available = true;
        for (i, in_buffer) in self.input_buffers.iter().enumerate().take(num_channels) {
            if let Some(block) = in_buffer.pop() {
                in_blocks[i] = block;
            } else {
                available = false;
                break;
            }
        }

        if available && num_channels > 0 {
            let block_len = in_blocks[0].len as usize;
            let mut in_slices_arr: [&[f32]; 16] = [&[]; 16];
            for i in 0..num_channels { in_slices_arr[i] = &in_blocks[i].data[..block_len]; }

            for (i, out_block) in out_blocks.iter_mut().enumerate().take(num_channels) {
                let mut context = ProcessContext {
                    transport: None,
                    host: None,
                    sub_block_offset: 0,
                    is_last_sub_block: true,
                };
                let mut out_slice = [&mut out_block.data[..block_len]];
                self.processor.process(&[in_slices_arr[i]], &mut out_slice, &mut context);
                out_block.len = block_len as u32;
                let _ = self.output_buffers[i].push(*out_block);
            }
        }
    }

    pub fn run_loop(&mut self) {
        loop {
            if let Some(efd) = &self.event_fd {
                let count = efd.wait();
                for _ in 0..count {
                    self.process_once();
                }
            } else {
                if !self.signal.check_and_clear() {
                    std::thread::yield_now();
                    continue;
                }
                self.process_once();
            }
        }
    }
}
