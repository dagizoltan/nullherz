use ipc_layer::ShmRingBuffer;
use audio_core::AudioProcessor;

/// A sidecar DSP process context.
pub struct SidecarContext<P: AudioProcessor> {
    processor: P,
    command_buffer: &'static ShmRingBuffer<control_plane::Command>,
}

impl<P: AudioProcessor> SidecarContext<P> {
    pub unsafe fn new(
        processor: P,
        command_ptr: *const ShmRingBuffer<control_plane::Command>,
    ) -> Self {
        Self {
            processor,
            command_buffer: &*command_ptr,
        }
    }

    /// Process one iteration of the sidecar loop.
    pub fn process_once(&mut self) {
        while let Some(cmd) = self.command_buffer.pop() {
            self.processor.apply_command(&cmd);
        }
    }

    /// Run the sidecar loop.
    ///
    /// Note: This current implementation uses a yield loop.
    /// For production, use a wait mechanism like `eventfd` or `futex`
    /// to avoid high CPU usage while maintaining low latency.
    pub fn run_loop(&mut self) {
        loop {
            self.process_once();

            // TODO: Wait for signal from host engine
            // eventfd_read(fd, &mut val);

            std::thread::yield_now();
        }
    }
}
