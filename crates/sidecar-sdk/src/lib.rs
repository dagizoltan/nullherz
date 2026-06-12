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
        let (cmd_layout, _) = ShmRingBuffer::<control_plane::Command>::layout(64);
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
        let cmd_rb_ptr = self.shm_cmd.ptr() as *const ShmRingBuffer<control_plane::Command>;
        let signal_ptr = self.shm_signal.ptr() as *const ShmSignal;

        let mut input_ptrs = Vec::new();
        for shm in &self.shm_inputs { input_ptrs.push(shm.ptr() as *const ShmRingBuffer<AudioBlock>); }

        let mut output_ptrs = Vec::new();
        for shm in &self.shm_outputs { output_ptrs.push(shm.ptr() as *const ShmRingBuffer<AudioBlock>); }

        let mut context = unsafe {
            SidecarContext::new(
                processor,
                cmd_rb_ptr,
                None, // Feedback SHM management could be added here
                input_ptrs,
                output_ptrs,
                signal_ptr,
                self.event_fd.take()
            )
        };

        context.run_loop();
    }
}

pub struct SidecarContext<P: AudioProcessor> {
    processor: P,
    command_buffer: &'static ShmRingBuffer<control_plane::Command>,
    feedback_buffer: Option<&'static ShmRingBuffer<control_plane::SidecarMetadata>>,
    input_buffers: Vec<&'static ShmRingBuffer<AudioBlock>>,
    output_buffers: Vec<&'static ShmRingBuffer<AudioBlock>>,
    signal: &'static ShmSignal,
    event_fd: Option<EventFd>,
}

impl<P: AudioProcessor> SidecarContext<P> {
    /// # Safety
    /// All pointers must be valid and point to pre-allocated shared memory structures.
    pub unsafe fn new(
        processor: P,
        command_ptr: *const ShmRingBuffer<control_plane::Command>,
        feedback_ptr: Option<*const ShmRingBuffer<control_plane::SidecarMetadata>>,
        inputs: Vec<*const ShmRingBuffer<AudioBlock>>,
        outputs: Vec<*const ShmRingBuffer<AudioBlock>>,
        signal_ptr: *const ShmSignal,
        event_fd: Option<EventFd>,
    ) -> Self {
        unsafe {
            Self {
                processor,
                command_buffer: &*command_ptr,
                feedback_buffer: feedback_ptr.map(|p| &*p),
                input_buffers: inputs.into_iter().map(|p| &*p).collect(),
                output_buffers: outputs.into_iter().map(|p| &*p).collect(),
                signal: &*signal_ptr,
                event_fd,
            }
        }
    }

    pub fn report_metadata(&self, metadata: control_plane::SidecarMetadata) {
        if let Some(fb) = self.feedback_buffer {
            let _ = fb.push(metadata);
        }
    }

    /// Process one iteration of the sidecar loop.
    pub fn process_once(&mut self) {
        self.signal.pulse_heartbeat();
        while let Some(cmd) = self.command_buffer.pop() {
            self.processor.apply_command(&cmd);
        }

        let mut in_blocks = [AudioBlock { data: [0.0; ipc_layer::MAX_BLOCK_SIZE], len: 0 }; 16];
        let mut out_blocks = [AudioBlock { data: [0.0; ipc_layer::MAX_BLOCK_SIZE], len: 0 }; 16];
        let num_channels = self.input_buffers.len().min(self.output_buffers.len()).min(16);

        let mut available = true;
        for (i, in_block) in in_blocks.iter_mut().enumerate().take(num_channels) {
            if let Some(block) = self.input_buffers[i].pop() {
                *in_block = block;
            } else {
                available = false;
                break;
            }
        }

        if available && num_channels > 0 {
            let block_len = in_blocks[0].len as usize;
            let mut in_slices_arr: [&[f32]; 16] = [&[]; 16];
            for i in 0..num_channels { in_slices_arr[i] = &in_blocks[i].data[..block_len]; }

            let mut out_ptrs: [*mut f32; 16] = [std::ptr::null_mut(); 16];
            for i in 0..num_channels { out_ptrs[i] = out_blocks[i].data.as_mut_ptr(); }

            let mut out_slices_reconstructed: [&mut [f32]; 16] = std::array::from_fn(|i| {
                if i < num_channels {
                    unsafe { std::slice::from_raw_parts_mut(out_ptrs[i], block_len) }
                } else {
                    &mut []
                }
            });

            let mut context = ProcessContext {

                transport: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };
            self.processor.process(&in_slices_arr[..num_channels], &mut out_slices_reconstructed[..num_channels], &mut context);

            for (i, out_block) in out_blocks.iter_mut().enumerate().take(num_channels) {
                out_block.len = block_len as u32;
                let _ = self.output_buffers[i].push(*out_block);
            }
        }
    }

    /// Run the sidecar loop.
    /// If an event_fd is present, it will perform a blocking wait to save CPU.
    /// Otherwise it will poll the ShmSignal and yield.
    pub fn run_loop(&mut self) {
        loop {
            if let Some(efd) = &self.event_fd {
                // Blocking wait (efficient)
                let count = efd.wait();
                for _ in 0..count {
                    self.process_once();
                }
            } else {
                // Polling wait (low latency but high CPU)
                if !self.signal.check_and_clear() {
                    std::thread::yield_now();
                    continue;
                }
                self.process_once();
            }
        }
    }
}
