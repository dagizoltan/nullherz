use ipc_layer::{ShmRingBuffer, AudioBlock};
use audio_core::AudioProcessor;

/// A sidecar DSP process context.
pub struct SidecarContext<P: AudioProcessor> {
    processor: P,
    command_buffer: &'static ShmRingBuffer<control_plane::Command>,
    input_buffers: Vec<&'static ShmRingBuffer<AudioBlock>>,
    output_buffers: Vec<&'static ShmRingBuffer<AudioBlock>>,
}

impl<P: AudioProcessor> SidecarContext<P> {
    pub unsafe fn new(
        processor: P,
        command_ptr: *const ShmRingBuffer<control_plane::Command>,
        inputs: Vec<*const ShmRingBuffer<AudioBlock>>,
        outputs: Vec<*const ShmRingBuffer<AudioBlock>>,
    ) -> Self {
        Self {
            processor,
            command_buffer: &*command_ptr,
            input_buffers: inputs.into_iter().map(|p| &*p).collect(),
            output_buffers: outputs.into_iter().map(|p| &*p).collect(),
        }
    }

    /// Process one iteration of the sidecar loop.
    pub fn process_once(&mut self) {
        while let Some(cmd) = self.command_buffer.pop() {
            self.processor.apply_command(&cmd);
        }

        let mut in_blocks = [AudioBlock { data: [0.0; 128] }; 16];
        let mut out_blocks = [AudioBlock { data: [0.0; 128] }; 16];
        let num_channels = self.input_buffers.len().min(self.output_buffers.len()).min(16);

        let mut available = true;
        for i in 0..num_channels {
            if let Some(block) = self.input_buffers[i].pop() {
                in_blocks[i] = block;
            } else {
                available = false;
                break;
            }
        }

        if available && num_channels > 0 {
            let mut in_slices_arr: [&[f32]; 16] = [&[]; 16];
            for i in 0..num_channels { in_slices_arr[i] = &in_blocks[i].data; }

            let mut out_ptrs: [*mut f32; 16] = [std::ptr::null_mut(); 16];
            for i in 0..num_channels { out_ptrs[i] = out_blocks[i].data.as_mut_ptr(); }

            let mut out_slices_reconstructed: [&mut [f32]; 16] = std::array::from_fn(|i| {
                if i < num_channels {
                    unsafe { std::slice::from_raw_parts_mut(out_ptrs[i], 128) }
                } else {
                    &mut []
                }
            });

            self.processor.process(&in_slices_arr[..num_channels], &mut out_slices_reconstructed[..num_channels]);

            for i in 0..num_channels {
                let _ = self.output_buffers[i].push(out_blocks[i]);
            }
        }
    }

    pub fn run_loop(&mut self) {
        loop {
            self.process_once();
            std::thread::yield_now();
        }
    }
}
