use nullherz_traits::{AudioProcessor, ProcessContext};
use audio_dsp::{DspKernel, EnvelopeFollower};

pub struct EnvelopeFollowerProcessor {
    kernel: EnvelopeFollower,
}

impl EnvelopeFollowerProcessor {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            kernel: EnvelopeFollower::new(sample_rate, 10.0, 100.0),
        }
    }
}

impl AudioProcessor for EnvelopeFollowerProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn reset(&mut self) {
        self.kernel.reset();
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        self.kernel.process(inputs, outputs);
    }

    fn set_parameter(&mut self, param_id: u32, value: f32, ramp_duration_samples: u32) {
        self.kernel.set_parameter(param_id, value, ramp_duration_samples);
    }
}
