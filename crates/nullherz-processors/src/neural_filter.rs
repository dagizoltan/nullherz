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
    nullherz_traits::ParamSpec { id: 0, name: "CUTOFF", min: 20.0, max: 20000.0, default: 1200.0 },
    nullherz_traits::ParamSpec { id: 1, name: "RESONANCE", min: 0.1, max: 10.0, default: 2.0 },
    nullherz_traits::ParamSpec { id: 2, name: "DRIVE", min: 0.0, max: 5.0, default: 0.5 },
];

/// Native Zero-Allocation Neural Dynamic Filter Processor
pub struct NeuralFilterProcessor {
    pub node_id: u64,
    pub cutoff: f32,
    pub resonance: f32,
    pub neural_drive: f32,
    s1: [f32; 16],
    s2: [f32; 16],
}

impl NeuralFilterProcessor {
    pub fn new(node_id: u64) -> Self {
        Self {
            node_id,
            cutoff: PARAMS[0].default,
            resonance: PARAMS[1].default,
            neural_drive: PARAMS[2].default,
            s1: [0.0; 16],
            s2: [0.0; 16],
        }
    }

    #[inline(always)]
    fn pade_tanh(x: f32) -> f32 {
        let x2 = x * x;
        let num = x * (1.0 + 0.12317192 * x2);
        let den = 1.0 + 0.4565311 * x2 + 0.01524316 * x2 * x2;
        (num / den).clamp(-1.0, 1.0)
    }
}

impl SignalProcessor for NeuralFilterProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], ctx: &mut ProcessContext) {
        let num_ch = inputs.len().min(outputs.len()).min(16);
        // The rate the DEVICE is running at, not a guess.
        //
        // This was `let sample_rate = 48000.0f32;` with `ctx` ignored, so the
        // cutoff was computed against a constant: a nominal 1 kHz corner landed
        // at ~1088 Hz on a 44.1 kHz device and ~500 Hz at 96 kHz. Same defect
        // the reverb had, and the same fix — read the transport, fall back only
        // when there is none (an `AddSource` topology mutation carries no
        // `ProcessContext`).
        let sample_rate = ctx
            .transport
            .as_ref()
            .map(|t| t.sample_rate)
            .filter(|r| *r > 0.0)
            .unwrap_or(nullherz_traits::DEFAULT_SAMPLE_RATE);
        let w0 = (std::f32::consts::TAU * self.cutoff / sample_rate).clamp(0.001, 3.0);
        let g = (w0 * 0.5).tan();
        let k = (1.0 / self.resonance).clamp(0.1, 2.0);

        for ch in 0..num_ch {
            let in_buf = inputs[ch];
            let out_buf = &mut outputs[ch];
            let n = in_buf.len().min(out_buf.len());

            let mut s1 = self.s1[ch];
            let mut s2 = self.s2[ch];

            for i in 0..n {
                let x = in_buf[i];
                // Neural hypernetwork non-linear feedback conditioning
                let feedback = Self::pade_tanh((s1 * k) * (1.0 + self.neural_drive * 0.5));
                let hp = (x - feedback - g * s1 - s2) / (1.0 + g * (g + k));
                let v1 = g * hp;
                let bp = v1 + s1;
                s1 = bp + v1;
                let v2 = g * bp;
                let lp = v2 + s2;
                s2 = lp + v2;

                out_buf[i] = lp;
            }

            self.s1[ch] = s1;
            self.s2[ch] = s2;
        }
    }

    fn reset(&mut self) {
        self.cutoff = 1200.0;
        self.resonance = 2.0;
        self.neural_drive = 0.5;
        self.s1.fill(0.0);
        self.s2.fill(0.0);
    }
}

impl MidiResponder for NeuralFilterProcessor {}
impl SnapshotProvider for NeuralFilterProcessor {}

impl AudioProcessor for NeuralFilterProcessor {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        // Range from PARAMS, which also REJECTS non-finite input. This used
        // to substitute 1.0 for a NaN, which clamped the cutoff to 20 Hz —
        // a garbage command silently moved the filter. Ignoring it leaves the
        // parameter where the operator put it.
        let Some(value) = nullherz_traits::ParamSpec::clamp_in(PARAMS, param_id, value) else {
            return;
        };
        match param_id {
            0 => self.cutoff = value,
            1 => self.resonance = value,
            2 => self.neural_drive = value,
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.cutoff,
            1 => self.resonance,
            2 => self.neural_drive,
            _ => 0.0,
        }
    }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { param_id, value, ramp_duration_samples, .. }) = command {
            self.set_parameter(*param_id, *value, *ramp_duration_samples);
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
    fn test_neural_filter_processing() {
        let mut proc = NeuralFilterProcessor::new(1);
        let in_l = vec![1.0f32; 64];
        let in_r = vec![-1.0f32; 64];
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
    }
}
