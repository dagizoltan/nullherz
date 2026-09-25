use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand,
};
use audio_dsp::simd_vec::FloatX16;

/// State tracker per audio channel
pub struct NeuralTcnState {
    history: [[f32; 64]; 16],
    write_ptr: usize,
    active_energy: f32,
}

impl NeuralTcnState {
    pub fn new() -> Self {
        Self {
            history: [[0.0; 64]; 16],
            write_ptr: 0,
            active_energy: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.history = [[0.0; 64]; 16];
        self.write_ptr = 0;
        self.active_energy = 0.0;
    }
}

impl Default for NeuralTcnState {
    fn default() -> Self {
        Self::new()
    }
}

/// Real-time Zero-Allocation Deep 4-Layer Dilated TCN Processor
/// Channel capacity C=16, Kernel K=3, Dilations d ∈ {1, 2, 4, 8}
pub struct NeuralTcnProcessor {
    pub node_id: u64,
    pub drive: f32,
    pub output_gain: f32,
    pub mix: f32,
    // Per-channel state history for independent stereo strips
    ch_state: [NeuralTcnState; 2],
    // Statically compiled weights
    w_in: [f32; 16],
    w_layer: [[[[f32; 3]; 16]; 16]; 4],
    b_layer: [[f32; 16]; 4],
    w_out: [f32; 16],
}

impl NeuralTcnProcessor {
    pub fn new(node_id: u64) -> Self {
        let mut w_in = [0.1f32; 16];
        for i in 0..16 { w_in[i] = 0.05 + 0.02 * (i as f32); }

        let mut w_layer = [[[[0.0f32; 3]; 16]; 16]; 4];
        let mut b_layer = [[0.01f32; 16]; 4];

        for l in 0..4 {
            for oc in 0..16 {
                b_layer[l][oc] = 0.005 * ((l + oc) as f32);
                for ic in 0..16 {
                    let base = 0.01 * (((l * 16 + oc + ic) % 7) as f32 - 3.0);
                    w_layer[l][oc][ic][0] = base;
                    w_layer[l][oc][ic][1] = if oc == ic { 0.8 } else { base * 0.5 };
                    w_layer[l][oc][ic][2] = base * 0.2;
                }
            }
        }

        let mut w_out = [0.0625f32; 16];
        for i in 0..16 { w_out[i] = 0.05 + 0.01 * ((i % 5) as f32); }

        Self {
            node_id,
            drive: 1.5,
            output_gain: 1.0,
            mix: 1.0,
            ch_state: [NeuralTcnState::new(), NeuralTcnState::new()],
            w_in,
            w_layer,
            b_layer,
            w_out,
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
        let abs_in = in_sample.abs();

        if abs_in < 1e-9 && st.active_energy < 1e-9 {
            return 0.0;
        }

        let drive_x = in_sample * self.drive;

        // 1. Input projection: 1 -> 16 hidden channels
        let mut h_current = [0.0f32; 16];
        for c in 0..16 {
            h_current[c] = drive_x * self.w_in[c];
        }

        let w_idx = st.write_ptr % 64;
        let mut new_energy = abs_in;
        for c in 0..16 {
            st.history[c][w_idx] = h_current[c];
            new_energy += h_current[c].abs();
        }
        st.active_energy = st.active_energy * 0.95 + new_energy * 0.05;

        // 2. Evaluate 4-layer Dilated TCN with dilations d = [1, 2, 4, 8]
        let dilations = [1, 2, 4, 8];

        for l in 0..4 {
            let d = dilations[l];
            let mut h_next = [0.0f32; 16];

            for oc in 0..16 {
                let mut acc_v = FloatX16::splat(0.0);

                for k in 0..3 {
                    let tap_offset = k * d;
                    let tap_idx = (st.write_ptr + 64 - tap_offset) % 64;

                    let mut tap_chans = [0.0f32; 16];
                    for ic in 0..16 {
                        tap_chans[ic] = st.history[ic][tap_idx];
                    }
                    let tap_v = FloatX16::new(tap_chans);

                    let mut w_chans = [0.0f32; 16];
                    for ic in 0..16 {
                        w_chans[ic] = self.w_layer[l][oc][ic][k];
                    }
                    let w_v = FloatX16::new(w_chans);

                    acc_v = acc_v + (tap_v * w_v);
                }

                // Sum across SIMD lanes and add single layer bias
                let acc = acc_v.reduce_sum() + self.b_layer[l][oc];

                // Non-linear Padé tanh activation
                h_next[oc] = Self::pade_tanh(acc);
            }

            // Update current layer state & history ring for layer cascade
            for c in 0..16 {
                h_current[c] = h_next[c];
                st.history[c][w_idx] = h_next[c];
            }
        }

        st.write_ptr = (st.write_ptr + 1) % 64;

        // 3. Output projection: 16 -> 1
        let mut out_acc = 0.0f32;
        for c in 0..16 {
            out_acc += h_current[c] * self.w_out[c];
        }

        let saturated = out_acc * self.output_gain;
        in_sample * (1.0 - self.mix) + saturated * self.mix
    }
}

impl SignalProcessor for NeuralTcnProcessor {
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
        self.drive = 1.5;
        self.output_gain = 1.0;
        self.mix = 1.0;
    }
}

impl MidiResponder for NeuralTcnProcessor {}
impl SnapshotProvider for NeuralTcnProcessor {}

impl AudioProcessor for NeuralTcnProcessor {
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
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { param_id, value, ramp_duration_samples, .. }) = command {
            self.set_parameter(*param_id, *value, *ramp_duration_samples);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neural_tcn_processing() {
        let mut proc = NeuralTcnProcessor::new(1);
        let in_l = vec![0.5f32; 128];
        let in_r = vec![-0.5f32; 128];
        let mut out_l = vec![0.0f32; 128];
        let mut out_r = vec![0.0f32; 128];

        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        proc.process(&[&in_l, &in_r], &mut [&mut out_l, &mut out_r], &mut ctx);

        assert!(out_l.iter().all(|s| s.is_finite()));
        assert!(out_r.iter().all(|s| s.is_finite()));
        assert!(out_l[0] != 0.0);
        assert!(out_r[0] != 0.0);
    }
}
