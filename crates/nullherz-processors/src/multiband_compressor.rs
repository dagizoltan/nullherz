use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand,
};

/// Zero-allocation 3-Band State-Space Model (SSM) Dynamic Compressor Processor.
pub struct MultiBandCompressorProcessor {
    pub threshold_db: f32,
    pub ratio: f32,
    pub crossover_low_hz: f32,
    pub crossover_high_hz: f32,
    svf_low: [[f32; 3]; 2],
    svf_high: [[f32; 3]; 2],
    env_states: [[f32; 3]; 2],
}

impl MultiBandCompressorProcessor {
    pub fn new() -> Self {
        Self {
            threshold_db: -12.0,
            ratio: 4.0,
            crossover_low_hz: 250.0,
            crossover_high_hz: 4000.0,
            svf_low: [[0.0; 3]; 2],
            svf_high: [[0.0; 3]; 2],
            env_states: [[0.0; 3]; 2],
        }
    }
}

impl Default for MultiBandCompressorProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalProcessor for MultiBandCompressorProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], ctx: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }

        let num_channels = inputs.len().min(outputs.len()).min(2);
        let block_len = inputs[0].len();
        let sr = ctx.transport.as_ref().map(|t| t.sample_rate).unwrap_or(48000.0).max(1000.0);

        let f_low = (std::f32::consts::PI * self.crossover_low_hz / sr).sin().clamp(0.001, 0.49);
        let f_high = (std::f32::consts::PI * self.crossover_high_hz / sr).sin().clamp(0.001, 0.49);
        let thresh_lin = 10.0f32.powf(self.threshold_db / 20.0);
        let attack = (-1.0 / (0.01 * sr)).exp();
        let release = (-1.0 / (0.1 * sr)).exp();

        for ch in 0..num_channels {
            let in_buf = inputs[ch];
            let out_buf = &mut outputs[ch][..block_len];

            let svf_l = &mut self.svf_low[ch];
            let svf_h = &mut self.svf_high[ch];
            let envs = &mut self.env_states[ch];

            for i in 0..block_len {
                let x = in_buf[i];
                if !x.is_finite() {
                    out_buf[i] = 0.0;
                    continue;
                }

                let hp_l = x - svf_l[0] - 0.707 * svf_l[1];
                let bp_l = f_low * hp_l + svf_l[1];
                let lp_l = f_low * bp_l + svf_l[0];
                svf_l[0] = lp_l;
                svf_l[1] = bp_l;

                let hp_h = x - svf_h[0] - 0.707 * svf_h[1];
                let bp_h = f_high * hp_h + svf_h[1];
                let lp_h = f_high * bp_h + svf_h[0];
                svf_h[0] = lp_h;
                svf_h[1] = bp_h;

                let band_low = lp_l;
                let band_high = hp_h;
                let band_mid = x - band_low - band_high;

                let bands = [band_low, band_mid, band_high];
                let mut compressed_sum = 0.0f32;

                for b in 0..3 {
                    let b_abs = bands[b].abs();
                    let alpha = if b_abs > envs[b] { attack } else { release };
                    envs[b] = alpha * envs[b] + (1.0 - alpha) * b_abs;

                    let gain = if envs[b] > thresh_lin {
                        let over_db = 20.0 * (envs[b] / thresh_lin.max(1e-6)).log10();
                        let gr_db = -over_db * (1.0 - 1.0 / self.ratio.max(1.0));
                        10.0f32.powf(gr_db / 20.0)
                    } else {
                        1.0
                    };

                    compressed_sum += bands[b] * gain;
                }

                out_buf[i] = compressed_sum;
            }
        }
    }

    fn reset(&mut self) {
        self.svf_low = [[0.0; 3]; 2];
        self.svf_high = [[0.0; 3]; 2];
        self.env_states = [[0.0; 3]; 2];
    }
}

impl MidiResponder for MultiBandCompressorProcessor {}
impl SnapshotProvider for MultiBandCompressorProcessor {}

impl AudioProcessor for MultiBandCompressorProcessor {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() {
            return;
        }
        match param_id {
            0 => self.threshold_db = value.clamp(-60.0, 0.0),
            1 => self.ratio = value.clamp(1.0, 20.0),
            2 => self.crossover_low_hz = value.clamp(40.0, 1000.0),
            3 => self.crossover_high_hz = value.clamp(1000.0, 15000.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.threshold_db,
            1 => self.ratio,
            2 => self.crossover_low_hz,
            3 => self.crossover_high_hz,
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
