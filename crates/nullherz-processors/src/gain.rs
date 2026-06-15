use nullherz_traits::AudioProcessor;

pub struct GainProcessor {
    gains: [audio_dsp::Gain; crate::MAX_CHANNELS],
    id: u64,
}

impl GainProcessor {
    pub fn new(id: u64, initial_gain: f32) -> Self {
        let gains = std::array::from_fn(|_| audio_dsp::Gain::new(initial_gain, 0.05));
        Self { gains, id }
    }
}

impl nullherz_traits::RtSafe for GainProcessor {}

impl AudioProcessor for GainProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        use audio_dsp::DspKernel;
        if inputs.is_empty() || outputs.is_empty() { return; }
        let num_channels = inputs.len().min(outputs.len()).min(crate::MAX_CHANNELS);
        for (i, gain) in self.gains.iter_mut().enumerate().take(num_channels) {
            gain.process(&inputs[i..i+1], &mut outputs[i..i+1]);
        }
    }
    fn set_parameter(&mut self, param_id: u32, value: f32, ramp_duration_samples: u32) {
        use audio_dsp::DspKernel;
        for g in self.gains.iter_mut() {
            g.set_parameter(param_id, value, ramp_duration_samples);
        }
    }

    fn reset(&mut self) {
        // audio_dsp::Gain doesn't have internal state like delay lines,
        // but we can reset current_gain to target_gain if we want bit-exact reset during ramps.
        for g in self.gains.iter_mut() {
            g.current_gain = g.target_gain;
            g.ramp_remaining = 0;
        }
    }

    fn apply_command(&mut self, command: &control_plane::Command) {
        match *command {
            control_plane::Command::SetParam { target_id, param_id, value, ramp_duration_samples }
                if target_id == self.id =>
            {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
            _ => {}
        }
    }
}
