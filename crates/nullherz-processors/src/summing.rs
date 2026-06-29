use nullherz_traits::AudioProcessor;

pub struct SummingProcessor {
    pub id: u64,
    inner: audio_dsp::SummingNode,
}

impl SummingProcessor {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            inner: audio_dsp::SummingNode::new(),
        }
    }
}

impl nullherz_traits::SignalProcessor for SummingProcessor {
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if outputs.is_empty() { return; }
        self.inner.process_16_to_1_simd(inputs, outputs[0]);
    }
}

impl nullherz_traits::MidiResponder for SummingProcessor { }

impl nullherz_traits::SnapshotProvider for SummingProcessor { }

impl AudioProcessor for SummingProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        if let nullherz_traits::Command::SetParam { target_id, param_id, value, ramp_duration_samples } = *command
            && target_id == self.id {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
    }
fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        if param_id == 0 {
            self.inner.set_gain(value);
        }
    }
fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 2.0,
            default: 1.0,
        }; 16];

        let name = b"Master Gain";
        parameters[0].name[..name.len()].copy_from_slice(name);

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: 0,
            num_parameters: 1,
            parameters,
        })
    }
}
