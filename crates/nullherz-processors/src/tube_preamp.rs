use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand,
};

/// Zero-allocation Tube Preamp & Transformer Saturation Processor.
pub struct TubePreampProcessor {
    pub drive: f32,
    pub bias: f32,
    pub output_gain: f32,
    dc_block_state: [f32; 2],
    transformer_state: [f32; 2],
}

impl TubePreampProcessor {
    pub fn new() -> Self {
        Self {
            drive: 1.0,
            bias: 0.0,
            output_gain: 1.0,
            dc_block_state: [0.0; 2],
            transformer_state: [0.0; 2],
        }
    }
}

impl Default for TubePreampProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalProcessor for TubePreampProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }

        let num_channels = inputs.len().min(outputs.len()).min(2);
        let block_len = inputs[0].len();
        let drive = self.drive.clamp(0.1, 10.0);
        let bias = self.bias.clamp(-0.5, 0.5);
        let out_gain = self.output_gain.clamp(0.0, 4.0);

        for ch in 0..num_channels {
            let in_buf = inputs[ch];
            let out_buf = &mut outputs[ch][..block_len];

            let dc_state = &mut self.dc_block_state[ch];
            let tf_state = &mut self.transformer_state[ch];

            for i in 0..block_len {
                let x_raw = in_buf[i];
                if !x_raw.is_finite() {
                    out_buf[i] = 0.0;
                    continue;
                }

                let x_biased = x_raw * drive + bias;

                let x_clamped = x_biased.clamp(-3.0, 3.0);
                let x2 = x_clamped * x_clamped;
                let num = x_clamped * (1.0 + 0.12317192 * x2);
                let den = 1.0 + 0.4565311 * x2 + 0.01524316 * x2 * x2;
                let saturated = num / den.max(1e-6);

                let tf_in = saturated + 0.05 * (*tf_state);
                let tf_out = tf_in - 0.02 * tf_in * tf_in.abs();
                *tf_state = tf_out;

                let dc_clean = tf_out - *dc_state + 0.995 * (*dc_state);
                *dc_state = tf_out;

                out_buf[i] = dc_clean * out_gain;
            }
        }
    }

    fn reset(&mut self) {
        self.dc_block_state = [0.0; 2];
        self.transformer_state = [0.0; 2];
    }
}

impl MidiResponder for TubePreampProcessor {}
impl SnapshotProvider for TubePreampProcessor {}

impl AudioProcessor for TubePreampProcessor {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() {
            return;
        }
        match param_id {
            0 => self.drive = value.clamp(0.1, 10.0),
            1 => self.bias = value.clamp(-0.5, 0.5),
            2 => self.output_gain = value.clamp(0.0, 4.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.drive,
            1 => self.bias,
            2 => self.output_gain,
            _ => 0.0,
        }
    }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { param_id, value, ramp_duration_samples, .. }) = command {
            self.set_parameter(*param_id, *value, *ramp_duration_samples);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
