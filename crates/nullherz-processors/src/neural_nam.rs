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
    nullherz_traits::ParamSpec { id: 0, name: "DRIVE", min: 0.0, max: 50.0, default: 2.0 },
    nullherz_traits::ParamSpec { id: 1, name: "OUTPUT", min: 0.0, max: 10.0, default: 0.8 },
    nullherz_traits::ParamSpec { id: 2, name: "MIX", min: 0.0, max: 1.0, default: 1.0 },
];

pub struct NamChannelState {
    // 10-sample history ring per channel for 10-layer FIR/IIR wave-shaping
    history: [f32; 32],
    write_ptr: usize,
}

impl NamChannelState {
    pub fn new() -> Self {
        Self {
            history: [0.0; 32],
            write_ptr: 0,
        }
    }

    pub fn reset(&mut self) {
        self.history.fill(0.0);
        self.write_ptr = 0;
    }
}

impl Default for NamChannelState {
    fn default() -> Self {
        Self::new()
    }
}

/// Real-time Zero-Allocation Neural Amp Modeler (NAM) Preamp / Amp Processor
/// 10-layer non-linear wave-shaping network for tube preamp and guitar amp emulation.
pub struct NeuralNamProcessor {
    pub node_id: u64,
    pub drive: f32,
    pub output_gain: f32,
    pub mix: f32,
    ch_state: [NamChannelState; 2],
    layer_weights: [f32; 10],
    layer_biases: [f32; 10],
}

impl NeuralNamProcessor {
    pub fn new(node_id: u64) -> Self {
        let mut layer_weights = [0.8f32; 10];
        let mut layer_biases = [0.01f32; 10];

        for i in 0..10 {
            let idx = i as f32;
            layer_weights[i] = 0.75 + 0.05 * (idx % 4.0);
            layer_biases[i] = 0.002 * (idx - 4.5);
        }

        Self {
            node_id,
            drive: PARAMS[0].default,
            output_gain: PARAMS[1].default,
            mix: PARAMS[2].default,
            ch_state: [NamChannelState::new(), NamChannelState::new()],
            layer_weights,
            layer_biases,
        }
    }

    #[inline(always)]
    fn pade_tanh(x: f32) -> f32 {
        let x2 = x * x;
        let num = x * (1.0 + 0.12317192 * x2);
        let den = 1.0 + 0.4565311 * x2 + 0.01524316 * x2 * x2;
        (num / den).clamp(-1.0, 1.0)
    }

    #[inline(always)]
    fn process_sample_channel(&mut self, ch: usize, in_sample: f32) -> f32 {
        let st = &mut self.ch_state[ch];

        if in_sample.abs() < 1e-9 {
            let mut energy = 0.0f32;
            for h in 0..32 {
                energy += st.history[h].abs();
            }
            if energy < 1e-9 {
                return 0.0;
            }
        }

        let drive_x = in_sample * self.drive;

        let w_idx = st.write_ptr % 32;
        st.history[w_idx] = drive_x;

        let mut x = drive_x;

        // Execute 10 non-linear neural layers sequentially
        for l in 0..10 {
            let tap_offset = l % 8;
            let tap_idx = (st.write_ptr + 32 - tap_offset) % 32;
            let hist_tap = st.history[tap_idx];

            let layer_in = x * self.layer_weights[l] + hist_tap * 0.1 + self.layer_biases[l];
            x = Self::pade_tanh(layer_in);
        }

        st.write_ptr = (st.write_ptr + 1) % 32;

        let saturated = x * self.output_gain;
        in_sample * (1.0 - self.mix) + saturated * self.mix
    }
}

impl SignalProcessor for NeuralNamProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext) {
        let num_ch = inputs.len().min(outputs.len()).min(2);
        if num_ch == 0 { return; }

        let block_len = inputs[0].len().min(outputs[0].len());

        for ch in 0..num_ch {
            let in_buf = inputs[ch];
            let out_buf = &mut outputs[ch];
            let n = in_buf.len().min(out_buf.len()).min(block_len);

            for i in 0..n {
                out_buf[i] = self.process_sample_channel(ch, in_buf[i]);
            }
        }
    }

    fn reset(&mut self) {
        for st in &mut self.ch_state {
            st.reset();
        }
        self.drive = 2.0;
        self.output_gain = 0.8;
        self.mix = 1.0;
    }
}

impl MidiResponder for NeuralNamProcessor {}
impl SnapshotProvider for NeuralNamProcessor {}

impl AudioProcessor for NeuralNamProcessor {
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
    fn test_neural_nam_processing() {
        let mut proc = NeuralNamProcessor::new(1);
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
    }
}
