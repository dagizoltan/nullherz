use nullherz_traits::{AudioProcessor, ProcessContext, Command, ProcessorCommand};
use audio_dsp::DspKernel;

/// A generic wrapper that implements `AudioProcessor` for any type implementing `DspKernel`.
/// This helps decouple engine traits from the raw signal math in `audio-dsp`.
pub struct DspKernelProcessor<K: DspKernel + 'static> {
    pub kernel: K,
    pub id: u64,
}

impl<K: DspKernel + 'static> DspKernelProcessor<K> {
    pub fn new(id: u64, kernel: K) -> Self {
        Self { kernel, id }
    }
}

impl<K: DspKernel + 'static> AudioProcessor for DspKernelProcessor<K> {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        self.kernel.process(inputs, outputs);
    }

    fn set_parameter(&mut self, param_id: u32, value: f32, ramp_duration_samples: u32) {
        self.kernel.set_parameter(param_id, value, ramp_duration_samples);
    }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Command::SetParam { target_id, param_id, value, ramp_duration_samples } = *command {
            if target_id == self.id {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
        }
    }

    fn reset(&mut self) {
        self.kernel.reset();
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

/// A multi-channel version of `DspKernelProcessor` that applies a single kernel type to multiple channels.
pub struct MultiChannelDspProcessor<K: DspKernel + 'static> {
    pub kernels: Vec<K>,
    pub id: u64,
}

impl<K: DspKernel + Clone + 'static> MultiChannelDspProcessor<K> {
    pub fn new(id: u64, kernel_template: K, num_channels: usize) -> Self {
        Self {
            kernels: vec![kernel_template; num_channels],
            id,
        }
    }
}

impl<K: DspKernel + 'static> AudioProcessor for MultiChannelDspProcessor<K> {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        let num_ch = inputs.len().min(outputs.len()).min(self.kernels.len());
        for i in 0..num_ch {
            self.kernels[i].process(&inputs[i..i+1], &mut outputs[i..i+1]);
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32, ramp_duration_samples: u32) {
        for k in &mut self.kernels {
            k.set_parameter(param_id, value, ramp_duration_samples);
        }
    }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Command::SetParam { target_id, param_id, value, ramp_duration_samples } = *command {
            if target_id == self.id {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
        }
    }

    fn reset(&mut self) {
        for k in &mut self.kernels {
            k.reset();
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
