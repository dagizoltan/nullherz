use ipc_layer::{ShmRingBuffer, AudioBlock, ShmSignal, EventFd, SharedMemory};
use nullherz_traits::AudioProcessor;
use std::sync::Arc;

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
    // Keep SHM segments alive to prevent use-after-free
    _shm_cmd: Option<Arc<SharedMemory>>,
    _shm_feedback: Option<Arc<SharedMemory>>,
    _shm_inputs: Vec<Arc<SharedMemory>>,
    _shm_outputs: Vec<Arc<SharedMemory>>,
    _shm_signal: Option<Arc<SharedMemory>>,
}

unsafe impl Send for SidecarProcessor {}

impl SidecarProcessor {
    /// # Safety
    /// All pointers must be valid and point to pre-allocated shared memory structures.
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
        input_shm[..num_channels].copy_from_slice(&inputs[..num_channels]);
        output_shm[..num_channels].copy_from_slice(&outputs[..num_channels]);
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
            _shm_cmd: None,
            _shm_feedback: None,
            _shm_inputs: Vec::new(),
            _shm_outputs: Vec::new(),
            _shm_signal: None,
        }
    }

    pub fn set_shm_references(
        &mut self,
        cmd: Arc<SharedMemory>,
        fb: Option<Arc<SharedMemory>>,
        inputs: Vec<Arc<SharedMemory>>,
        outputs: Vec<Arc<SharedMemory>>,
        signal: Arc<SharedMemory>,
    ) {
        self._shm_cmd = Some(cmd);
        self._shm_feedback = fb;
        self._shm_inputs = inputs;
        self._shm_outputs = outputs;
        self._shm_signal = Some(signal);
    }

    pub fn poll_feedback(&self) -> Option<control_plane::SidecarMetadata> {
        self.feedback_consumer_ptr.and_then(|ptr| unsafe { (*ptr).pop() })
    }
}

impl AudioProcessor for SidecarProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn reset(&mut self) {
        self.last_heartbeat = 0;
        self.missed_deadline_count = 0;
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        let current_heartbeat = unsafe { (*self.signal).get_heartbeat() };
        // Use wrapping subtraction to detect progress robustly across u64 wrap.
        // We detect stall if heartbeat hasn't changed since last block AND we've initialized (last_heartbeat != 0).
        let is_stalled = current_heartbeat.wrapping_sub(self.last_heartbeat) == 0 && self.last_heartbeat != 0;

        // Hardening: If we've missed too many deadlines, force a local bypass immediately.
        const STALL_THRESHOLD: u32 = 10;
        let force_bypass = is_stalled || self.missed_deadline_count > STALL_THRESHOLD;

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
                if !force_bypass {
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
                    if i == 0 {
                        self.missed_deadline_count = self.missed_deadline_count.saturating_add(1);
                    }
                } else if i == 0 {
                    // Reset stall counter if we successfully consumed a block
                    self.missed_deadline_count = 0;
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

#[cfg(test)]
mod tests {
    use super::*;
    use ipc_layer::AudioBlock;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_sidecar_processor_bypass_on_stall() {
        let cmd_shm = SharedMemory::create("nullherz_test_cmd", 4096).unwrap();
        let sig_shm = SharedMemory::create("nullherz_test_sig", 4096).unwrap();
        let in_shm = SharedMemory::create("nullherz_test_in", 4096).unwrap();
        let out_shm = SharedMemory::create("nullherz_test_out", 4096).unwrap();

        unsafe {
            let cmd_ptr = ShmRingBuffer::<control_plane::Command>::init(cmd_shm.ptr(), 16);
            let sig_ptr = sig_shm.ptr() as *mut ShmSignal;
            std::ptr::write(sig_ptr, ShmSignal::new());
            let in_ptr = ShmRingBuffer::<AudioBlock>::init(in_shm.ptr(), 16);
            let out_ptr = ShmRingBuffer::<AudioBlock>::init(out_shm.ptr(), 16);

            let mut proc = SidecarProcessor::new(
                cmd_ptr,
                None,
                &[in_ptr],
                &[out_ptr as *const _],
                sig_ptr,
                None
            );

            let input = vec![1.0f32; 128];
            let mut output = vec![0.0f32; 128];
            let mut context = nullherz_traits::ProcessContext {
                transport: None,
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };

            // First call - no heartbeat yet, should bypass (or pop if available, but here it's empty)
            proc.process(&[&input], &mut [&mut output], &mut context);
            assert_eq!(output[0], 1.0); // Bypass

            // Update heartbeat but leave output buffer empty
            (*sig_ptr).pulse_heartbeat();
            proc.process(&[&input], &mut [&mut output], &mut context);
            assert_eq!(output[0], 1.0); // Still bypass because output ringbuffer empty

            // Stall detection: don't update heartbeat
            proc.last_heartbeat = 10;
            (*sig_ptr).heartbeat.store(10, Ordering::Relaxed);
            proc.process(&[&input], &mut [&mut output], &mut context);
            assert_eq!(proc.missed_deadline_count, 3); // 3 total calls so far, all bypass/missed
        }
    }
}
