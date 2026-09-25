use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand,
};

const MAX_MOD_BUF: usize = 4096;

/// Zero-allocation Algorithmic Modulation FX Processor.
pub struct AlgorithmicModulationProcessor {
    pub mode: u32,
    pub rate_hz: f32,
    pub depth: f32,
    pub feedback: f32,
    pub wet_dry: f32,

    lfo_phase: f32,
    delay_buffers: [[f32; MAX_MOD_BUF]; 2],
    delay_write_pos: [usize; 2],
    phaser_states: [[f32; 4]; 2],
}

impl AlgorithmicModulationProcessor {
    pub fn new() -> Self {
        Self {
            mode: 0,
            rate_hz: 1.5,
            depth: 0.5,
            feedback: 0.3,
            wet_dry: 0.5,
            lfo_phase: 0.0,
            delay_buffers: [[0.0; MAX_MOD_BUF]; 2],
            delay_write_pos: [0; 2],
            phaser_states: [[0.0; 4]; 2],
        }
    }
}

impl Default for AlgorithmicModulationProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalProcessor for AlgorithmicModulationProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], ctx: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }

        let num_channels = inputs.len().min(outputs.len()).min(2);
        let block_len = inputs[0].len();
        let sr = ctx.transport.as_ref().map(|t| t.sample_rate).unwrap_or(48000.0).max(1000.0);

        let lfo_inc = 2.0 * std::f32::consts::PI * self.rate_hz.clamp(0.01, 20.0) / sr;
        let depth = self.depth.clamp(0.0, 1.0);
        let fb = self.feedback.clamp(0.0, 0.95);
        let wet = self.wet_dry.clamp(0.0, 1.0);
        let dry = 1.0 - wet;

        for i in 0..block_len {
            self.lfo_phase = (self.lfo_phase + lfo_inc) % (2.0 * std::f32::consts::PI);
            let lfo_val = 0.5 + 0.5 * self.lfo_phase.sin();

            for ch in 0..num_channels {
                let x = inputs[ch][i];
                if !x.is_finite() {
                    outputs[ch][i] = 0.0;
                    continue;
                }

                let mod_signal = match self.mode {
                    0 => {
                        let base_delay = sr * 0.015;
                        let mod_delay = base_delay + depth * (sr * 0.010) * lfo_val;
                        let write_pos = self.delay_write_pos[ch];
                        let read_pos = (write_pos + MAX_MOD_BUF - (mod_delay as usize % MAX_MOD_BUF)) % MAX_MOD_BUF;

                        let delayed = self.delay_buffers[ch][read_pos];
                        self.delay_buffers[ch][write_pos] = x + delayed * fb;
                        self.delay_write_pos[ch] = (write_pos + 1) % MAX_MOD_BUF;

                        delayed
                    }
                    1 => {
                        let base_delay = sr * 0.001;
                        let mod_delay = base_delay + depth * (sr * 0.004) * lfo_val;
                        let write_pos = self.delay_write_pos[ch];
                        let read_pos = (write_pos + MAX_MOD_BUF - (mod_delay as usize % MAX_MOD_BUF)) % MAX_MOD_BUF;

                        let delayed = self.delay_buffers[ch][read_pos];
                        self.delay_buffers[ch][write_pos] = x + delayed * fb;
                        self.delay_write_pos[ch] = (write_pos + 1) % MAX_MOD_BUF;

                        delayed
                    }
                    _ => {
                        let ap_a = (0.2 + 0.6 * lfo_val * depth).clamp(0.05, 0.95);
                        let mut ap_in = x;
                        for stage in 0..4 {
                            let y = -ap_a * ap_in + self.phaser_states[ch][stage];
                            self.phaser_states[ch][stage] = ap_in + ap_a * y;
                            ap_in = y;
                        }
                        ap_in
                    }
                };

                outputs[ch][i] = x * dry + mod_signal * wet;
            }
        }
    }

    fn reset(&mut self) {
        self.delay_buffers = [[0.0; MAX_MOD_BUF]; 2];
        self.delay_write_pos = [0; 2];
        self.phaser_states = [[0.0; 4]; 2];
        self.lfo_phase = 0.0;
    }
}

impl MidiResponder for AlgorithmicModulationProcessor {}
impl SnapshotProvider for AlgorithmicModulationProcessor {}

impl AudioProcessor for AlgorithmicModulationProcessor {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() {
            return;
        }
        match param_id {
            0 => self.mode = (value as u32).min(2),
            1 => self.rate_hz = value.clamp(0.01, 20.0),
            2 => self.depth = value.clamp(0.0, 1.0),
            3 => self.feedback = value.clamp(0.0, 0.95),
            4 => self.wet_dry = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.mode as f32,
            1 => self.rate_hz,
            2 => self.depth,
            3 => self.feedback,
            4 => self.wet_dry,
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
