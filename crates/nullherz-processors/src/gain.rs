use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorCommand};
use crate::dsp_kernel_processor::MultiChannelDspProcessor;

pub struct GainProcessor {
    inner: MultiChannelDspProcessor<audio_dsp::Gain>,
}

impl GainProcessor {
    pub fn new(id: u64, initial_gain: f32) -> Self {
        let gain = audio_dsp::Gain::new(initial_gain, 0.05);
        Self {
            inner: MultiChannelDspProcessor::new(id, gain, nullherz_traits::MAX_CHANNELS),
        }
    }
}

impl nullherz_traits::RtSafe for GainProcessor {}

impl AudioProcessor for GainProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn set_safe_mode(&mut self, enabled: bool) {
        if enabled {
            self.inner.set_parameter(0, 1.0, 0); // Neutral gain in safe mode
        }
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext) {
        self.inner.process(inputs, outputs, context);
    }
    fn set_parameter(&mut self, param_id: u32, value: f32, ramp_duration_samples: u32) {
        self.inner.set_parameter(param_id, value, ramp_duration_samples);
    }

    fn reset(&mut self) {
        self.inner.reset();
    }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        self.inner.apply_command(command);
    }

    fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 2.0,
            default: 1.0,
        }; 16];

        let name = b"Gain";
        parameters[0].name[..name.len()].copy_from_slice(name);

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.inner.id,
            num_parameters: 1,
            parameters,
        })
    }
}
