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

impl AudioProcessor for GainProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        let num_channels = inputs.len().min(outputs.len()).min(crate::MAX_CHANNELS);
        for (i, gain) in self.gains.iter_mut().enumerate().take(num_channels) {
            gain.process_block(inputs[i], outputs[i]);
        }
    }
    fn apply_command(&mut self, command: &control_plane::Command) {
        match *command {
            control_plane::Command::SetParam { target_id, param_id, value, ramp_duration_samples }
                if target_id == self.id && param_id == 0 =>
            {
                for g in self.gains.iter_mut() {
                    g.set_gain(value, ramp_duration_samples);
                }
            }
            _ => {}
        }
    }
}
