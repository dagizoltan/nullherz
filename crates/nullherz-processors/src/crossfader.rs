use nullherz_traits::AudioProcessor;

pub struct CrossfaderProcessor {
    pub id: u64,
    inner: audio_dsp::Crossfader,
}

impl CrossfaderProcessor {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            inner: audio_dsp::Crossfader::new(),
        }
    }
}

impl nullherz_traits::SignalProcessor for CrossfaderProcessor {
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if inputs.len() < 2 || outputs.is_empty() { return; }
        self.inner.process_block_simd(inputs[0], inputs[1], outputs[0]);
    }
fn reset(&mut self) {}
}

impl nullherz_traits::MidiResponder for CrossfaderProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for CrossfaderProcessor { }

impl AudioProcessor for CrossfaderProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, ramp_duration_samples }) = *command
            && target_id == self.id {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
    }
fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        if param_id == 0 {
            self.inner.set_position(value);
        } else if param_id == 1 {
            self.inner.set_curve(value);
        }
    }
fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 1.0,
            default: 0.5,
        }; 16];

        let name0 = b"Position";
        parameters[0].id = 0;
        parameters[0].name[..name0.len()].copy_from_slice(name0);

        let name1 = b"Curve";
        parameters[1].id = 1;
        parameters[1].name[..name1.len()].copy_from_slice(name1);

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 2,
            parameters,
        })
    }
}
