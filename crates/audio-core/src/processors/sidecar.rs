use ipc_layer::{ShmRingBuffer, AudioBlock, ShmSignal, EventFd};
use crate::processors::AudioProcessor;

pub const MAX_CHANNELS: usize = 16;

pub struct SidecarProcessor {
    command_producer_ptr: *const ShmRingBuffer<control_plane::Command>,
    feedback_consumer_ptr: Option<*const ShmRingBuffer<control_plane::SidecarMetadata>>,
    pub last_metadata: Option<control_plane::SidecarMetadata>,
    input_shm: [*mut ShmRingBuffer<AudioBlock>; MAX_CHANNELS],
    output_shm: [*const ShmRingBuffer<AudioBlock>; MAX_CHANNELS],
    num_channels: usize,
    signal: *const ShmSignal,
    event_fd: Option<EventFd>,
    last_heartbeat: u64,
    missed_deadline_count: u32,
}

unsafe impl Send for SidecarProcessor {}

impl SidecarProcessor {
    pub unsafe fn new(
        command_ptr: *const ShmRingBuffer<control_plane::Command>,
        feedback_ptr: Option<*const ShmRingBuffer<control_plane::SidecarMetadata>>,
        inputs: &[*mut ShmRingBuffer<AudioBlock>],
        outputs: &[*const ShmRingBuffer<AudioBlock>],
        signal: *const ShmSignal,
        event_fd: Option<EventFd>,
    ) -> Self {
        let mut input_shm = [std::ptr::null_mut(); MAX_CHANNELS];
        let mut output_shm = [std::ptr::null(); MAX_CHANNELS];
        let num_channels = inputs.len().min(MAX_CHANNELS).min(outputs.len());
        for i in 0..num_channels { input_shm[i] = inputs[i]; output_shm[i] = outputs[i]; }
        Self {
            command_producer_ptr: command_ptr,
            feedback_consumer_ptr: feedback_ptr,
            last_metadata: None,
            input_shm,
            output_shm,
            num_channels,
            signal,
            event_fd,
            last_heartbeat: 0,
            missed_deadline_count: 0,
        }
    }

    pub fn poll_feedback(&self) -> Option<control_plane::SidecarMetadata> {
        self.feedback_consumer_ptr.and_then(|ptr| unsafe { (*ptr).pop() })
    }
}

impl AudioProcessor for SidecarProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut crate::processors::ProcessContext) {
        let current_heartbeat = unsafe { (*self.signal).get_heartbeat() };
        let is_stalled = current_heartbeat == self.last_heartbeat && self.last_heartbeat != 0;

        for i in 0..self.num_channels {
            if i < inputs.len() {
                let mut block = AudioBlock { data: [0.0; ipc_layer::MAX_BLOCK_SIZE], len: 0 };
                let len = inputs[i].len().min(ipc_layer::MAX_BLOCK_SIZE);
                block.data[..len].copy_from_slice(&inputs[i][..len]);
                block.len = len as u32;
                unsafe { let _ = (*self.input_shm[i]).push(block); }
            }

            if i < outputs.len() {
                let mut consumed = false;
                if !is_stalled {
                    unsafe {
                        if let Some(block) = (*self.output_shm[i]).pop() {
                            let len = outputs[i].len().min(block.len as usize);
                            outputs[i][..len].copy_from_slice(&block.data[..len]);
                            consumed = true;
                        }
                    }
                }

                if !consumed {
                    // Fail-safe: Bypass or Silence on stall/missed deadline
                    if i < inputs.len() {
                        outputs[i].copy_from_slice(inputs[i]); // Bypass
                    } else {
                        outputs[i].fill(0.0); // Silence
                    }
                    if i == 0 { self.missed_deadline_count += 1; }
                }
            }
        }

        self.last_heartbeat = current_heartbeat;
        unsafe { (*self.signal).notify(); }
        if let Some(efd) = &self.event_fd { efd.notify(); }
    }
    fn apply_command(&mut self, command: &control_plane::Command) {
        unsafe {
            let _ = (*self.command_producer_ptr).push(*command);
            (*self.signal).notify();
        }
        if let Some(efd) = &self.event_fd { efd.notify(); }
    }
}
