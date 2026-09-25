use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand,
};

/// Native Zero-Allocation Neural Saturation & Analog Preamp Processor
pub struct NeuralSaturatorProcessor {
    pub node_id: u64,
    pub drive: f32,
    pub output_gain: f32,
    pub mix: f32,
}

impl NeuralSaturatorProcessor {
    pub fn new(node_id: u64) -> Self {
        Self {
            node_id,
            drive: 1.5,
            output_gain: 1.0,
            mix: 1.0,
        }
    }

    /// Rational Padé Approximant for tanh(x):
    /// tanh(x) approx x * (1 + 0.12317192 * x^2) / (1 + 0.4565311 * x^2 + 0.01524316 * x^4)
    #[inline(always)]
    fn pade_tanh(x: f32) -> f32 {
        let x2 = x * x;
        let num = x * (1.0 + 0.12317192 * x2);
        let den = 1.0 + 0.4565311 * x2 + 0.01524316 * x2 * x2;
        (num / den).clamp(-1.0, 1.0)
    }
}

impl SignalProcessor for NeuralSaturatorProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext) {
        let num_ch = inputs.len().min(outputs.len());
        let drive = self.drive;
        let gain = self.output_gain;
        let mix = self.mix;

        for ch in 0..num_ch {
            let in_buf = inputs[ch];
            let out_buf = &mut outputs[ch];
            let n = in_buf.len().min(out_buf.len());

            for i in 0..n {
                let x = in_buf[i] * drive;
                let sat = Self::pade_tanh(x) * gain;
                out_buf[i] = in_buf[i] * (1.0 - mix) + sat * mix;
            }
        }
    }

    fn reset(&mut self) {
        self.drive = 1.5;
        self.output_gain = 1.0;
        self.mix = 1.0;
    }
}

impl MidiResponder for NeuralSaturatorProcessor {}
impl SnapshotProvider for NeuralSaturatorProcessor {}

impl AudioProcessor for NeuralSaturatorProcessor {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        let safe_val = if value.is_finite() { value } else { 1.0 };
        match param_id {
            0 => self.drive = safe_val.clamp(0.0, 50.0),
            1 => self.output_gain = safe_val.clamp(0.0, 10.0),
            2 => self.mix = safe_val.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.drive,
            1 => self.output_gain,
            2 => self.mix,
            _ => 0.0,
        }
    }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        match command {
            nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetBpm(bpm)) => {
                let bpm_val = if bpm.is_finite() { *bpm } else { 120.0 };
                self.drive = (bpm_val / 120.0).clamp(0.5, 3.0);
            }
            nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { param_id, value, ramp_duration_samples, .. }) => {
                self.set_parameter(*param_id, *value, *ramp_duration_samples);
            }
            _ => {}
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neural_saturator_processing() {
        let mut proc = NeuralSaturatorProcessor::new(1);
        let in_l = vec![0.5f32; 64];
        let in_r = vec![-0.5f32; 64];
        let mut out_l = vec![0.0f32; 64];
        let mut out_r = vec![0.0f32; 64];

        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        proc.process(&[&in_l, &in_r], &mut [&mut out_l, &mut out_r], &mut ctx);

        assert!(out_l.iter().all(|s| s.is_finite()));
        assert!(out_r.iter().all(|s| s.is_finite()));
        assert!(out_l[0] > 0.0);
        assert!(out_r[0] < 0.0);
    }
}
