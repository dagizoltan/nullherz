use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand,
};

/// Zero-allocation HyperNetwork Conditioned Parametric EQ Processor.
pub struct HyperNetworkEqProcessor {
    pub gain_db: f32,
    pub freq_hz: f32,
    pub q_factor: f32,
    x_state: [[f32; 2]; 2],
    y_state: [[f32; 2]; 2],
    coeffs: [f32; 5],
}

impl HyperNetworkEqProcessor {
    pub fn new() -> Self {
        let mut proc = Self {
            gain_db: 0.0,
            freq_hz: 1000.0,
            q_factor: 0.707,
            x_state: [[0.0; 2]; 2],
            y_state: [[0.0; 2]; 2],
            coeffs: [1.0, 0.0, 0.0, 0.0, 0.0],
        };
        proc.recompute_coefficients(48000.0);
        proc
    }

    fn recompute_coefficients(&mut self, sample_rate: f32) {
        let sr = sample_rate.max(1000.0);
        let omega = 2.0 * std::f32::consts::PI * self.freq_hz.clamp(20.0, sr * 0.49) / sr;
        let sin_w = omega.sin();
        let cos_w = omega.cos();
        let alpha = sin_w / (2.0 * self.q_factor.max(0.1));
        let a = 10.0f32.powf(self.gain_db / 40.0);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w;
        let a2 = 1.0 - alpha / a;

        let inv_a0 = 1.0 / a0.max(1e-6);
        self.coeffs = [
            b0 * inv_a0,
            b1 * inv_a0,
            b2 * inv_a0,
            a1 * inv_a0,
            a2 * inv_a0,
        ];
    }
}

impl Default for HyperNetworkEqProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalProcessor for HyperNetworkEqProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], ctx: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }

        let sample_rate = ctx.transport.as_ref().map(|t| t.sample_rate).unwrap_or(48000.0);
        self.recompute_coefficients(sample_rate);

        let num_channels = inputs.len().min(outputs.len()).min(2);
        let block_len = inputs[0].len();
        let [b0, b1, b2, a1, a2] = self.coeffs;

        for ch in 0..num_channels {
            let in_buf = inputs[ch];
            let out_buf = &mut outputs[ch][..block_len];

            let [x1, x2] = &mut self.x_state[ch];
            let [y1, y2] = &mut self.y_state[ch];

            for i in 0..block_len {
                let x = in_buf[i];
                if !x.is_finite() {
                    out_buf[i] = 0.0;
                    continue;
                }

                let y = b0 * x + b1 * (*x1) + b2 * (*x2) - a1 * (*y1) - a2 * (*y2);
                let y_clean = if y.is_finite() { y } else { 0.0 };

                *x2 = *x1;
                *x1 = x;
                *y2 = *y1;
                *y1 = y_clean;

                let x_clamped = y_clean.clamp(-3.0, 3.0);
                let x2_sq = x_clamped * x_clamped;
                let num = x_clamped * (1.0 + 0.12317192 * x2_sq);
                let den = 1.0 + 0.4565311 * x2_sq + 0.01524316 * x2_sq * x2_sq;
                out_buf[i] = num / den.max(1e-6);
            }
        }
    }

    fn reset(&mut self) {
        self.x_state = [[0.0; 2]; 2];
        self.y_state = [[0.0; 2]; 2];
    }
}

impl MidiResponder for HyperNetworkEqProcessor {}
impl SnapshotProvider for HyperNetworkEqProcessor {}

impl AudioProcessor for HyperNetworkEqProcessor {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() {
            return;
        }
        match param_id {
            0 => self.gain_db = value.clamp(-24.0, 24.0),
            1 => self.freq_hz = value.clamp(20.0, 20000.0),
            2 => self.q_factor = value.clamp(0.1, 10.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.gain_db,
            1 => self.freq_hz,
            2 => self.q_factor,
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
