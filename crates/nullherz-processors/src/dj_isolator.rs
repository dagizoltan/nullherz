use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorCommand, Command};

/// 3-band DJ isolator (kill EQ) that processes stereo L/R together in SIMD
/// lanes — a direct processor rather than the per-channel
/// `MultiChannelDspProcessor` wrapper, so both channels share one
/// register-resident crossover pass. Bit-identical on finite input to two
/// scalar `DjIsolator`s (proven in audio-dsp's
/// `test_dj_isolator_stereo_matches_two_scalar_bitexact`).
pub struct DjIsolatorProcessor {
    pub id: u64,
    inner: audio_dsp::DjIsolatorStereo,
}

impl DjIsolatorProcessor {
    pub fn new(id: u64, sample_rate: f32) -> Self {
        Self { id, inner: audio_dsp::DjIsolatorStereo::with_sample_rate(sample_rate) }
    }
}

impl nullherz_traits::RtSafe for DjIsolatorProcessor {}

impl nullherz_traits::SignalProcessor for DjIsolatorProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        if inputs.len() >= 2 && outputs.len() >= 2 {
            // Stereo: split the two output channels and run L/R in SIMD lanes.
            let (l, r) = outputs.split_at_mut(1);
            self.inner.process_stereo(inputs[0], inputs[1], l[0], r[0]);
        } else {
            // Mono wiring / conformance: lane-0-only path.
            self.inner.process_mono(inputs[0], outputs[0]);
        }
    }

    fn reset(&mut self) {
        self.inner.reset();
    }

    fn set_safe_mode(&mut self, enabled: bool) {
        // Neutral (unity) bands in safe mode — matches the old kernel reset.
        if enabled {
            self.inner.gains = [1.0, 1.0, 1.0];
            self.inner.reset();
        }
    }
}

impl nullherz_traits::MidiResponder for DjIsolatorProcessor {
    fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) {}
}

impl nullherz_traits::SnapshotProvider for DjIsolatorProcessor {}

impl AudioProcessor for DjIsolatorProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        // Params 0/1/2 = low/mid/high band gain, matching the scalar kernel.
        self.inner.set_gain(param_id as usize, value);
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        if (param_id as usize) < 3 { self.inner.gains[param_id as usize] } else { 0.0 }
    }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, ramp_duration_samples }) = *command
            && target_id == self.id {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
    }

    fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 10.0,
            default: 1.0,
        }; 16];

        let names: &[&[u8]] = &[b"Low", b"Mid", b"High"];
        for (i, &name) in names.iter().enumerate() {
            parameters[i].id = i as u32;
            parameters[i].name[..name.len()].copy_from_slice(name);
        }

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 3,
            parameters,
        })
    }

    fn processor_type(&self) -> &'static str { "dj_isolator" }
}
