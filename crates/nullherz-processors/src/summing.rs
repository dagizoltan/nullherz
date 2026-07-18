use nullherz_traits::AudioProcessor;

pub struct SummingProcessor {
    pub id: u64,
    inner: audio_dsp::SummingNode,
    soft_clip: bool,
    threshold: f32,
}

impl SummingProcessor {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            inner: audio_dsp::SummingNode::new(),
            soft_clip: true,
            threshold: 1.0,
        }
    }
}

impl nullherz_traits::SignalProcessor for SummingProcessor {
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if outputs.is_empty() { return; }
        self.inner.process_16_to_1_simd(inputs, outputs[0]);

        if self.soft_clip {
            let threshold = self.threshold;
            let inv_threshold = 1.0 / threshold;
            let out = &mut outputs[0];
            let mut i = 0;
            while i + 4 <= out.len() {
                let v = audio_dsp::simd_vec::load_f32x4(out, i);
                let clipped = audio_dsp::simd_vec::tanh_simd(v * wide::f32x4::from(inv_threshold)) * wide::f32x4::from(threshold);
                audio_dsp::simd_vec::store_f32x4(out, i, clipped);
                i += 4;
            }
            while i < out.len() {
                out[i] = (out[i] * inv_threshold).tanh() * threshold;
                i += 1;
            }
        }
    }
}

impl nullherz_traits::MidiResponder for SummingProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for SummingProcessor { }

impl AudioProcessor for SummingProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, ramp_duration_samples }) = *command
            && target_id == self.id {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
    }
fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() { return; }
        match param_id {
            0 => self.inner.set_gain(value.clamp(0.0, 4.0)),
            1 => self.soft_clip = value > 0.5,
            2 => self.threshold = value.max(0.01),
            _ => {}
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

        let names: &[&[u8]] = &[b"Master Gain", b"SoftClip", b"Threshold"];
        let mins = [0.0, 0.0, 0.01];
        let maxs = [4.0, 1.0, 2.0];
        let defs = [1.0, 1.0, 1.0];

        for (i, &name) in names.iter().enumerate() {
            parameters[i].id = i as u32;
            parameters[i].name[..name.len()].copy_from_slice(name);
            parameters[i].min = mins[i];
            parameters[i].max = maxs[i];
            parameters[i].default = defs[i];
        }

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 3,
            parameters,
        })
    }
}
