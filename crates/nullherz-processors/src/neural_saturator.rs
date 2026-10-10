use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand,
};

/// This processor's parameters, declared once.
///
/// `metadata()` publishes this table and `set_parameter` clamps against it, so
/// the range a host draws is the range the processor enforces. Stating it twice
/// is what let 31 parameters across 10 processors drift from their own clamps
/// (`declared_params_match_behaviour_test`).
///
/// Ids are part of the saved-project format — a project references
/// `(processor_type, param_id)`. Append, never renumber.
const PARAMS: &[nullherz_traits::ParamSpec] = &[
    nullherz_traits::ParamSpec { id: 0, name: "DRIVE", min: 0.0, max: 50.0, default: 1.5 },
    nullherz_traits::ParamSpec { id: 1, name: "OUTPUT", min: 0.0, max: 10.0, default: 1.0 },
    nullherz_traits::ParamSpec { id: 2, name: "MIX", min: 0.0, max: 1.0, default: 1.0 },
];

/// Native Zero-Allocation Neural Saturation & Analog Preamp Processor
pub struct NeuralSaturatorProcessor {
    pub node_id: u64,
    pub drive: f32,
    pub output_gain: f32,
    pub mix: f32,
    pub oversamplers: Vec<audio_dsp::util::Oversampler2x>,
}

impl NeuralSaturatorProcessor {
    pub fn new(node_id: u64) -> Self {
        Self {
            node_id,
            drive: PARAMS[0].default,
            output_gain: PARAMS[1].default,
            mix: PARAMS[2].default,
            oversamplers: (0..16).map(|_| audio_dsp::util::Oversampler2x::new()).collect(),
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

            if ch < self.oversamplers.len() {
                self.oversamplers[ch].process_block(in_buf, out_buf, |s| {
                    let x = s * drive;
                    let sat = Self::pade_tanh(x) * gain;
                    s * (1.0 - mix) + sat * mix
                });
            } else {
                let n = in_buf.len().min(out_buf.len());
                for i in 0..n {
                    let x = in_buf[i] * drive;
                    let sat = Self::pade_tanh(x) * gain;
                    out_buf[i] = in_buf[i] * (1.0 - mix) + sat * mix;
                }
            }
        }
    }

    fn reset(&mut self) {
        self.drive = 1.5;
        self.output_gain = 1.0;
        self.mix = 1.0;
        self.oversamplers = (0..16).map(|_| audio_dsp::util::Oversampler2x::new()).collect();
    }
}

impl MidiResponder for NeuralSaturatorProcessor {}
impl SnapshotProvider for NeuralSaturatorProcessor {}

impl AudioProcessor for NeuralSaturatorProcessor {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        // Range from PARAMS, which also REJECTS non-finite input. This
        // used to substitute 1.0 for a NaN, so a garbage command moved
        // the parameter instead of being ignored.
        let Some(value) = nullherz_traits::ParamSpec::clamp_in(PARAMS, param_id, value) else {
            return;
        };
        match param_id {
            0 => self.drive = value,
            1 => self.output_gain = value,
            2 => self.mix = value,
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

    fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        // Same table `set_parameter` clamps against.
        Some(nullherz_traits::ProcessorMetadata::from_specs(0, PARAMS))
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
