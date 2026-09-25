use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand,
};

pub struct SsmChannelState {
    // 24 hidden states
    state: [f32; 24],
}

impl SsmChannelState {
    pub fn new() -> Self {
        Self { state: [0.0; 24] }
    }

    pub fn reset(&mut self) {
        self.state.fill(0.0);
    }
}

impl Default for SsmChannelState {
    fn default() -> Self {
        Self::new()
    }
}

/// Real-time Zero-Allocation Diagonal State-Space Model (SSM) Neural Compressor
/// Implements state evolution x_{k+1} = A x_k + B u_k, y_k = C x_k + D u_k with 24 hidden states.
pub struct NeuralSsmCompressor {
    pub node_id: u64,
    pub threshold_db: f32,
    pub ratio: f32,
    pub mix: f32,
    // Per-channel state history for independent stereo processing
    ch_state: [SsmChannelState; 2],
    // State space parameters (24-wide hidden dimension)
    a_diag: [f32; 24],
    b_vec: [f32; 24],
    c_vec: [f32; 24],
    d_scalar: f32,
}

impl NeuralSsmCompressor {
    pub fn new(node_id: u64) -> Self {
        let mut a_diag = [0.9f32; 24];
        let mut b_vec = [0.1f32; 24];
        let mut c_vec = [0.1f32; 24];

        for i in 0..24 {
            let idx = i as f32;
            a_diag[i] = 0.85 + 0.005 * (idx % 10.0);
            b_vec[i] = 0.05 + 0.01 * (idx % 6.0);
            c_vec[i] = 0.08 - 0.002 * (idx % 8.0);
        }

        Self {
            node_id,
            threshold_db: -12.0,
            ratio: 4.0,
            mix: 1.0,
            ch_state: [SsmChannelState::new(), SsmChannelState::new()],
            a_diag,
            b_vec,
            c_vec,
            d_scalar: 0.1,
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
    fn process_sample_channel(&mut self, ch: usize, u: f32) -> f32 {
        let st = &mut self.ch_state[ch];

        // 1. State update: x_{k+1} = A x_k + B u_k
        let mut state_sum = 0.0f32;
        for i in 0..24 {
            let next_x = st.state[i] * self.a_diag[i] + u * self.b_vec[i];
            st.state[i] = next_x;
            state_sum += next_x * self.c_vec[i];
        }

        // 2. Non-linear activation & gain reduction response
        let net_out = Self::pade_tanh(state_sum + u * self.d_scalar);

        // Dynamic gain reduction based on threshold
        let thresh_linear = 10.0f32.powf(self.threshold_db / 20.0);
        let envelope = u.abs().max(net_out.abs());

        let gain_reduction = if envelope > thresh_linear {
            let over_db = 20.0 * (envelope / thresh_linear).log10();
            let gr_db = -over_db * (1.0 - 1.0 / self.ratio);
            10.0f32.powf(gr_db / 20.0)
        } else {
            1.0
        };

        let compressed = u * gain_reduction;
        u * (1.0 - self.mix) + compressed * self.mix
    }
}

impl SignalProcessor for NeuralSsmCompressor {
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
        self.threshold_db = -12.0;
        self.ratio = 4.0;
        self.mix = 1.0;
    }
}

impl MidiResponder for NeuralSsmCompressor {}
impl SnapshotProvider for NeuralSsmCompressor {}

impl AudioProcessor for NeuralSsmCompressor {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        let safe_val = if value.is_finite() { value } else { 0.0 };
        match param_id {
            0 => self.threshold_db = safe_val.clamp(-60.0, 0.0),
            1 => self.ratio = safe_val.clamp(1.0, 20.0),
            2 => self.mix = safe_val.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.threshold_db,
            1 => self.ratio,
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
    fn test_neural_ssm_compressor_processing() {
        let mut proc = NeuralSsmCompressor::new(1);
        let in_l = vec![0.8f32; 64];
        let in_r = vec![-0.8f32; 64];
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
