use audio_dsp::Filter;
use crate::traits::AudioProcessor;
use audio_dsp::{Gain, BiquadFilter, BiquadCoefficients, SimdBiquad, Crossfader, SummingNode};
use ipc_layer::{AudioBlock, ShmRingBuffer, ShmSignal, EventFd};

pub struct GainProcessor {
    gain: Gain,
    id: u64,
}
impl GainProcessor {
    pub fn new(id: u64, initial_gain: f32) -> Self {
        Self { gain: Gain::new(initial_gain, 0.05), id }
    }
}
impl AudioProcessor for GainProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        self.gain.process_block(inputs[0], outputs[0]);
    }
    fn apply_command(&mut self, command: &control_plane::Command) {
        if let control_plane::Command::SetParam { target_id, param_id, value, ramp_duration_samples } = command {
            if *target_id == self.id && *param_id == 0 {
                self.gain.set_gain(*value, *ramp_duration_samples);
            }
        }
    }
}

pub struct BiquadProcessor {
    filter: BiquadFilter,
    id: u64,
}
impl BiquadProcessor {
    pub fn new(id: u64, coeffs: BiquadCoefficients) -> Self {
        Self { filter: BiquadFilter::new(coeffs), id }
    }
}
impl AudioProcessor for BiquadProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx2") {
                unsafe { self.filter.process_block_simd(inputs[0], outputs[0]); }
                return;
            }
        }
        for i in 0..inputs[0].len() { outputs[0][i] = self.filter.process_sample(inputs[0][i]); }
    }
    fn apply_command(&mut self, command: &control_plane::Command) {
        if let control_plane::Command::SetParam { target_id, param_id, value, ramp_duration_samples } = command {
            if *target_id == self.id {
                let mut coeffs = self.filter.target_coeffs;
                match param_id {
                    0 => coeffs.b0 = *value,
                    1 => coeffs.b1 = *value,
                    2 => coeffs.b2 = *value,
                    3 => coeffs.a1 = *value,
                    4 => coeffs.a2 = *value,
                    _ => {}
                }
                self.filter.set_coeffs_ramped(coeffs, *ramp_duration_samples);
            }
        }
    }
}

pub struct SimdBiquadProcessor {
    inner: SimdBiquad,
}
impl SimdBiquadProcessor {
    pub fn new(coeffs: BiquadCoefficients) -> Self {
        Self { inner: SimdBiquad::new(coeffs) }
    }
}
impl AudioProcessor for SimdBiquadProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        let len = inputs[0].len();
        let num_channels = inputs.len().min(outputs.len()).min(16);
        if num_channels == 8 {
            let mut in_ptrs = [std::ptr::null(); 8];
            let mut out_ptrs = [std::ptr::null_mut(); 8];
            for i in 0..8 { in_ptrs[i] = inputs[i].as_ptr(); out_ptrs[i] = outputs[i].as_mut_ptr(); }
            #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
            unsafe { self.inner.process_8_channels(in_ptrs, out_ptrs, len); }
        } else if num_channels == 16 {
            let mut in_ptrs = [std::ptr::null(); 16];
            let mut out_ptrs = [std::ptr::null_mut(); 16];
            for i in 0..16 { in_ptrs[i] = inputs[i].as_ptr(); out_ptrs[i] = outputs[i].as_mut_ptr(); }
            #[cfg(target_arch = "x86_64")]
            unsafe { self.inner.process_16_channels(in_ptrs, out_ptrs, len); }
        } else {
            for ch in 0..num_channels { self.inner.process_scalar(ch, inputs[ch], outputs[ch]); }
        }
    }
}

pub struct CrossfaderProcessor {
    inner: Crossfader,
}
impl CrossfaderProcessor {
    pub fn new() -> Self { Self { inner: Crossfader::new() } }
}
impl AudioProcessor for CrossfaderProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.len() < 2 || outputs.is_empty() { return; }
        self.inner.process_block(inputs[0], inputs[1], outputs[0]);
    }
}

pub struct SummingProcessor {
    inner: SummingNode,
}
impl SummingProcessor {
    pub fn new() -> Self { Self { inner: SummingNode::new() } }
}
impl AudioProcessor for SummingProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if outputs.is_empty() { return; }
        self.inner.process_16_to_1(inputs, outputs[0]);
    }
}

/// A proxy processor that delegates DSP execution to an external process.
///
/// `SidecarProcessor` communicates with autonomous sidecar processes via
/// high-performance shared memory segments. Commands and multi-channel audio data
/// are exchanged using lock-free SPSC ring buffers, satisfying the engine's
/// zero-allocation and zero-syscall real-time constraints.
///
/// Signaling is handled via `ShmSignal` (atomic flags) and `EventFd` for efficient
/// blocking waits in the sidecar process.
pub struct SidecarProcessor {
    pub(crate) command_producer_ptr: *const ShmRingBuffer<control_plane::Command>,
    pub(crate) feedback_consumer_ptr: Option<*const ShmRingBuffer<control_plane::SidecarMetadata>>,
    pub last_metadata: Option<control_plane::SidecarMetadata>,
    pub(crate) input_shm: [*mut ShmRingBuffer<AudioBlock>; 16],
    pub(crate) output_shm: [*const ShmRingBuffer<AudioBlock>; 16],
    pub(crate) num_channels: usize,
    pub(crate) signal: *const ShmSignal,
    pub(crate) event_fd: Option<EventFd>,
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
        let mut input_shm = [std::ptr::null_mut(); 16];
        let mut output_shm = [std::ptr::null(); 16];
        let num_channels = inputs.len().min(16).min(outputs.len());
        for i in 0..num_channels { input_shm[i] = inputs[i]; output_shm[i] = outputs[i]; }
        Self { command_producer_ptr: command_ptr, feedback_consumer_ptr: feedback_ptr, last_metadata: None, input_shm, output_shm, num_channels, signal, event_fd }
    }
    pub fn poll_feedback(&self) -> Option<control_plane::SidecarMetadata> {
        self.feedback_consumer_ptr.and_then(|ptr| unsafe { (*ptr).pop() })
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
