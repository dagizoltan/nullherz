use nullherz_traits::{AudioProcessor, ProcessContext};
use audio_dsp::{DspKernel, EnvelopeFollower};

pub struct EnvelopeFollowerProcessor {
    pub id: u64,
    kernel: EnvelopeFollower,
}

impl EnvelopeFollowerProcessor {
    pub fn new(id: u64, sample_rate: f32) -> Self {
        Self {
            id,
            kernel: EnvelopeFollower::new(sample_rate, 10.0, 100.0),
        }
    }
}

impl nullherz_traits::SignalProcessor for EnvelopeFollowerProcessor {
fn reset(&mut self) {
        self.kernel.reset();
    }
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        self.kernel.process(inputs, outputs);
    }
}

impl nullherz_traits::MidiResponder for EnvelopeFollowerProcessor { }

impl nullherz_traits::SnapshotProvider for EnvelopeFollowerProcessor { }

impl AudioProcessor for EnvelopeFollowerProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        if let nullherz_traits::Command::SetParam { target_id, param_id, value, ramp_duration_samples } = *command
            && target_id == self.id {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
    }
fn set_parameter(&mut self, param_id: u32, value: f32, ramp_duration_samples: u32) {
        self.kernel.set_parameter(param_id, value, ramp_duration_samples);
    }
fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.1,
            max: 1000.0,
            default: 10.0,
        }; 16];

        let name_attack = b"Attack";
        parameters[0].id = 0;
        parameters[0].name[..name_attack.len()].copy_from_slice(name_attack);

        let name_release = b"Release";
        parameters[1].id = 1;
        parameters[1].name[..name_release.len()].copy_from_slice(name_release);
        parameters[1].default = 100.0;

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: 0, // Should be passed in constructor if we want global tracking
            num_parameters: 2,
            parameters,
        })
    }
}
