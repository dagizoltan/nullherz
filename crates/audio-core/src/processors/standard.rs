use crate::processors::AudioProcessor;
use audio_dsp::Filter;

pub struct GainProcessor {
    gain: audio_dsp::Gain,
    id: u64,
}

impl GainProcessor {
    pub fn new(id: u64, initial_gain: f32) -> Self {
        Self { gain: audio_dsp::Gain::new(initial_gain, 0.05), id }
    }
}

impl AudioProcessor for GainProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        self.gain.process_block(inputs[0], outputs[0]);
    }
    fn apply_command(&mut self, command: &control_plane::Command) {
        if let control_plane::Command::SetParam { target_id, param_id, value, ramp_duration_samples } = command {
            if *target_id == self.id && *param_id == 0 {
                self.gain.set_gain(*value, *ramp_duration_samples);
            }
        }
    }
}

pub struct BiquadProcessor {
    filter: audio_dsp::BiquadFilter,
    id: u64,
}

impl BiquadProcessor {
    pub fn new(id: u64, coeffs: audio_dsp::BiquadCoefficients) -> Self {
        Self { filter: audio_dsp::BiquadFilter::new(coeffs), id }
    }
}

impl AudioProcessor for BiquadProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }

        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx2") {
                unsafe {
                    self.filter.process_block_simd(inputs[0], outputs[0]);
                }
                return;
            }
        }

        for i in 0..inputs[0].len() {
            outputs[0][i] = self.filter.process_sample(inputs[0][i]);
        }
    }

    fn apply_command(&mut self, command: &control_plane::Command) {
        if let control_plane::Command::SetParam { target_id, param_id, value, ramp_duration_samples } = command {
            if *target_id == self.id {
                let mut coeffs = self.filter.target_coeffs;
                match param_id {
                    0 => coeffs.b0 = *value,
                    1 => coeffs.b1 = *value,
                    2 => coeffs.b2 = *value,
                    3 => coeffs.a1 = *value,
                    4 => coeffs.a2 = *value,
                    _ => {}
                }
                self.filter.set_coeffs_ramped(coeffs, *ramp_duration_samples);
            }
        }
    }
}

pub struct SimdBiquadProcessor {
    inner: audio_dsp::SimdBiquad,
}

impl SimdBiquadProcessor {
    pub fn new(coeffs: audio_dsp::BiquadCoefficients) -> Self {
        Self { inner: audio_dsp::SimdBiquad::new(coeffs) }
    }
}

impl AudioProcessor for SimdBiquadProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        let len = inputs[0].len();
        let num_channels = inputs.len().min(outputs.len()).min(16);

        if num_channels == 8 {
            let mut in_ptrs = [std::ptr::null(); 8];
            let mut out_ptrs = [std::ptr::null_mut(); 8];
            for i in 0..8 {
                in_ptrs[i] = inputs[i].as_ptr();
                out_ptrs[i] = outputs[i].as_mut_ptr();
            }
            #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
            unsafe { self.inner.process_8_channels(in_ptrs, out_ptrs, len); }
        } else if num_channels == 16 {
            let mut in_ptrs = [std::ptr::null(); 16];
            let mut out_ptrs = [std::ptr::null_mut(); 16];
            for i in 0..16 {
                in_ptrs[i] = inputs[i].as_ptr();
                out_ptrs[i] = outputs[i].as_mut_ptr();
            }
            #[cfg(target_arch = "x86_64")]
            unsafe { self.inner.process_16_channels(in_ptrs, out_ptrs, len); }
        } else {
            for ch in 0..num_channels {
                self.inner.process_scalar(ch, inputs[ch], outputs[ch]);
            }
        }
    }
}

pub struct CrossfaderProcessor {
    inner: audio_dsp::Crossfader,
}

impl CrossfaderProcessor {
    pub fn new() -> Self { Self { inner: audio_dsp::Crossfader::new() } }
}

impl AudioProcessor for CrossfaderProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.len() < 2 || outputs.is_empty() { return; }
        self.inner.process_block(inputs[0], inputs[1], outputs[0]);
    }
}

pub struct SummingProcessor {
    inner: audio_dsp::SummingNode,
}

impl SummingProcessor {
    pub fn new() -> Self { Self { inner: audio_dsp::SummingNode::new() } }
}

impl AudioProcessor for SummingProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if outputs.is_empty() { return; }
        
        #[cfg(target_arch = "x86_64")]
        if is_x86_feature_detected!("avx2") {
            unsafe {
                self.inner.process_16_to_1_avx2(inputs, outputs[0]);
            }
            return;
        }

        self.inner.process_16_to_1(inputs, outputs[0]);
    }
}
