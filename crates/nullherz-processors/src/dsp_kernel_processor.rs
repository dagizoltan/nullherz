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

impl <K: DspKernel + 'static> nullherz_traits::SignalProcessor for DspKernelProcessor<K> {
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        self.kernel.process(inputs, outputs);
    }
fn reset(&mut self) {
        self.kernel.reset();
    }
fn set_safe_mode(&mut self, enabled: bool) {
        if enabled { self.reset(); }
    }
}

impl <K: DspKernel + 'static> nullherz_traits::MidiResponder for DspKernelProcessor<K> { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl <K: DspKernel + 'static> nullherz_traits::SnapshotProvider for DspKernelProcessor<K> { }

impl <K: DspKernel + 'static> AudioProcessor for DspKernelProcessor<K> {
fn set_parameter(&mut self, param_id: u32, value: f32, ramp_duration_samples: u32) {
        self.kernel.set_parameter(param_id, value, ramp_duration_samples);
    }
    fn get_parameter(&self, param_id: u32) -> f32 {
        self.kernel.get_parameter(param_id)
    }
fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Command::SetParam { target_id, param_id, value, ramp_duration_samples } = *command
            && target_id == self.id {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
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

impl <K: DspKernel + 'static> nullherz_traits::SignalProcessor for MultiChannelDspProcessor<K> {
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        let num_ch = inputs.len().min(outputs.len()).min(self.kernels.len());
        for i in 0..num_ch {
            self.kernels[i].process(&inputs[i..i+1], &mut outputs[i..i+1]);
        }
    }
fn reset(&mut self) {
        for k in &mut self.kernels {
            k.reset();
        }
    }
fn set_safe_mode(&mut self, enabled: bool) {
        if enabled { self.reset(); }
    }
}

impl <K: DspKernel + 'static> nullherz_traits::MidiResponder for MultiChannelDspProcessor<K> { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl <K: DspKernel + 'static> nullherz_traits::SnapshotProvider for MultiChannelDspProcessor<K> { }

impl <K: DspKernel + 'static> AudioProcessor for MultiChannelDspProcessor<K> {
fn set_parameter(&mut self, param_id: u32, value: f32, ramp_duration_samples: u32) {
        for k in &mut self.kernels {
            k.set_parameter(param_id, value, ramp_duration_samples);
        }
    }
    fn get_parameter(&self, param_id: u32) -> f32 {
        if !self.kernels.is_empty() { self.kernels[0].get_parameter(param_id) } else { 0.0 }
    }
fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Command::SetParam { target_id, param_id, value, ramp_duration_samples } = *command
            && target_id == self.id {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
    }
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
