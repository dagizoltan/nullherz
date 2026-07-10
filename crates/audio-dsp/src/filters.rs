pub trait Filter {
    fn process_sample(&mut self, input: f32) -> f32;
}

#[derive(Debug, Clone, Copy)]
pub struct BiquadCoefficients {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

/// A Biquad Filter using Direct Form II Transposed.
#[derive(Debug, Clone, Copy)]
#[repr(C, align(64))]
pub struct BiquadFilter {
    pub coeffs: BiquadCoefficients,
    pub target_coeffs: BiquadCoefficients,
    pub ramp_duration: u32,
    pub ramp_counter: u32,
    pub(crate) b0_step: f32,
    pub(crate) b1_step: f32,
    pub(crate) b2_step: f32,
    pub(crate) a1_step: f32,
    pub(crate) a2_step: f32,
    pub(crate) z1: f32,
    pub(crate) z2: f32,
}

impl Default for BiquadCoefficients {
    fn default() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 }
    }
}

impl BiquadCoefficients {
    pub fn linkwitz_riley_lp(freq: f32, sample_rate: f32) -> Self {
        let omega = std::f32::consts::PI * freq / sample_rate;
        let theta = omega.tan();
        let k = theta.powi(2);
        let delta = k + 2.0_f32.sqrt() * theta + 1.0;

        Self {
            b0: k / delta,
            b1: 2.0 * k / delta,
            b2: k / delta,
            a1: 2.0 * (k - 1.0) / delta,
            a2: (k - 2.0_f32.sqrt() * theta + 1.0) / delta,
        }
    }

    pub fn linkwitz_riley_hp(freq: f32, sample_rate: f32) -> Self {
        let omega = std::f32::consts::PI * freq / sample_rate;
        let theta = omega.tan();
        let k = theta.powi(2);
        let delta = k + 2.0_f32.sqrt() * theta + 1.0;

        Self {
            b0: 1.0 / delta,
            b1: -2.0 / delta,
            b2: 1.0 / delta,
            a1: 2.0 * (k - 1.0) / delta,
            a2: (k - 2.0_f32.sqrt() * theta + 1.0) / delta,
        }
    }
}

impl BiquadFilter {
    pub fn new(coeffs: BiquadCoefficients) -> Self {
        Self {
            coeffs,
            target_coeffs: coeffs,
            ramp_duration: 0,
            ramp_counter: 0,
            b0_step: 0.0,
            b1_step: 0.0,
            b2_step: 0.0,
            a1_step: 0.0,
            a2_step: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn update_coeffs(&mut self, coeffs: BiquadCoefficients) {
        self.target_coeffs = coeffs;
        self.coeffs = coeffs;
        self.ramp_duration = 0;
        self.ramp_counter = 0;
        self.b0_step = 0.0;
        self.b1_step = 0.0;
        self.b2_step = 0.0;
        self.a1_step = 0.0;
        self.a2_step = 0.0;
    }

    pub fn set_coeffs_ramped(&mut self, coeffs: BiquadCoefficients, duration: u32) {
        if duration == 0 {
            self.update_coeffs(coeffs);
        } else {
            self.target_coeffs = coeffs;
            self.ramp_duration = duration;
            self.ramp_counter = duration;
            let inv_duration = 1.0 / duration as f32;
            self.b0_step = (coeffs.b0 - self.coeffs.b0) * inv_duration;
            self.b1_step = (coeffs.b1 - self.coeffs.b1) * inv_duration;
            self.b2_step = (coeffs.b2 - self.coeffs.b2) * inv_duration;
            self.a1_step = (coeffs.a1 - self.coeffs.a1) * inv_duration;
            self.a2_step = (coeffs.a2 - self.coeffs.a2) * inv_duration;
        }
    }

    /// Optimized block processing using scalar unrolling.
    pub fn process_block_unrolled(&mut self, input: &[f32], output: &mut [f32]) {
        let len = input.len();
        if len == 0 { return; }

        if self.ramp_duration > 0 {
            for i in 0..len {
                output[i] = self.process_sample(input[i]);
            }
            return;
        }

        let mut z1 = self.z1;
        let mut z2 = self.z2;
        let b0 = self.coeffs.b0;
        let b1 = self.coeffs.b1;
        let b2 = self.coeffs.b2;
        let a1 = self.coeffs.a1;
        let a2 = self.coeffs.a2;

        let mut i = 0;

        // SIMD unrolled path (4-lane)
        while i + 4 <= len {
            // Note: Since single-channel biquad has recursive dependencies (y depends on previous y),
            // a true vectorized path requires a specialized prefix-sum like approach.
            // For now, we utilize the SIMD lanes for independent filter stages or unroll scalar for throughput.
            unsafe {
                for _ in 0..4 {
                    let x = *input.get_unchecked(i);
                    let y = x * b0 + z1;
                    if !y.is_finite() {
                        z1 = 0.0; z2 = 0.0; *output.get_unchecked_mut(i) = 0.0;
                    } else {
                        z1 = x * b1 - y * a1 + z2;
                        z2 = x * b2 - y * a2;
                        *output.get_unchecked_mut(i) = y;
                    }
                    i += 1;
                }
            }
        }

        // Scalar fallback for small blocks
        while i < len {
            unsafe {
                let x = *input.get_unchecked(i);
                let mut y = x * b0 + z1;
                if !y.is_finite() {
                    y = 0.0;
                    z1 = 0.0;
                    z2 = 0.0;
                } else {
                    z1 = x * b1 - y * a1 + z2;
                    z2 = x * b2 - y * a2;
                }
                *output.get_unchecked_mut(i) = y;
            }
            i += 1;
        }

        self.z1 = z1;
        self.z2 = z2;
    }
}

impl Filter for BiquadFilter {
    fn process_sample(&mut self, input: f32) -> f32 {
        if self.ramp_counter > 0 {
            self.coeffs.b0 += self.b0_step;
            self.coeffs.b1 += self.b1_step;
            self.coeffs.b2 += self.b2_step;
            self.coeffs.a1 += self.a1_step;
            self.coeffs.a2 += self.a2_step;

            self.ramp_counter -= 1;
            if self.ramp_counter == 0 {
                self.coeffs = self.target_coeffs;
                self.ramp_duration = 0;
            }
        }

        let mut output = input * self.coeffs.b0 + self.z1;
        if !output.is_finite() {
            output = 0.0;
            self.z1 = 0.0;
            self.z2 = 0.0;
        } else {
            self.z1 = input * self.coeffs.b1 - output * self.coeffs.a1 + self.z2;
            self.z2 = input * self.coeffs.b2 - output * self.coeffs.a2;
        }
        output
    }
}

/// A Biquad Filter that processes 8 or 16 channels in parallel using AVX2/AVX-512.
#[derive(Clone)]
#[repr(C, align(64))]
pub struct SimdBiquad {
    pub coeffs: BiquadCoefficients,
    pub z1: [f32; 16],
    pub z2: [f32; 16],
}

impl crate::DspKernel for SimdBiquad {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        let num_ch = inputs.len().min(outputs.len());
        if num_ch >= 16 {
             let mut in_ptrs = [std::ptr::null(); 16];
             let mut out_ptrs = [std::ptr::null_mut(); 16];
             for i in 0..16 {
                 in_ptrs[i] = inputs[i].as_ptr();
                 out_ptrs[i] = outputs[i].as_mut_ptr();
             }
             self.process_16_channels(in_ptrs, out_ptrs, inputs[0].len());
        } else if num_ch >= 8 {
            let mut in_ptrs = [std::ptr::null(); 8];
            let mut out_ptrs = [std::ptr::null_mut(); 8];
            for i in 0..8 {
                in_ptrs[i] = inputs[i].as_ptr();
                out_ptrs[i] = outputs[i].as_mut_ptr();
            }
            self.process_8_channels(in_ptrs, out_ptrs, inputs[0].len());
        } else if num_ch >= 4 {
            let mut in_ptrs = [std::ptr::null(); 4];
            let mut out_ptrs = [std::ptr::null_mut(); 4];
            for i in 0..4 {
                in_ptrs[i] = inputs[i].as_ptr();
                out_ptrs[i] = outputs[i].as_mut_ptr();
            }
            self.process_4_channels(in_ptrs, out_ptrs, inputs[0].len());
        } else {
            for i in 0..num_ch {
                self.process_scalar(i, inputs[i], outputs[i]);
            }
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32, _ramp_samples: u32) {
        // Simple implementation for now
        match id {
            0 => self.coeffs.b0 = value,
            1 => self.coeffs.b1 = value,
            2 => self.coeffs.b2 = value,
            3 => self.coeffs.a1 = value,
            4 => self.coeffs.a2 = value,
            _ => {}
        }
    }
}

impl SimdBiquad {
    pub fn new(coeffs: BiquadCoefficients) -> Self {
        Self {
            coeffs,
            z1: [0.0; 16],
            z2: [0.0; 16],
        }
    }

    pub fn process_scalar(&mut self, channel: usize, input: &[f32], output: &mut [f32]) {
        let mut z1 = self.z1[channel];
        let mut z2 = self.z2[channel];
        for i in 0..input.len() {
            let mut out = input[i] * self.coeffs.b0 + z1;
            if !out.is_finite() {
                out = 0.0;
                z1 = 0.0;
                z2 = 0.0;
            } else {
                z1 = input[i] * self.coeffs.b1 - out * self.coeffs.a1 + z2;
                z2 = input[i] * self.coeffs.b2 - out * self.coeffs.a2;
            }
            output[i] = out;
        }
        self.z1[channel] = z1;
        self.z2[channel] = z2;
    }

    pub fn process_16_channels(&mut self, inputs: [*const f32; 16], outputs: [*mut f32; 16], len: usize) {
        use crate::simd_vec::*;

        let b0 = FloatX16::from(self.coeffs.b0);
        let b1 = FloatX16::from(self.coeffs.b1);
        let b2 = FloatX16::from(self.coeffs.b2);
        let a1 = FloatX16::from(self.coeffs.a1);
        let a2 = FloatX16::from(self.coeffs.a2);

        let mut z1 = unsafe { load_f32x16_ptr(self.z1.as_ptr()) };
        let mut z2 = unsafe { load_f32x16_ptr(self.z2.as_ptr()) };

        for i in 0..len {
            let mut in_arr = [0.0f32; 16];
            for ch in 0..16 {
                unsafe { in_arr[ch] = *inputs[ch].add(i) };
            }
            let x = FloatX16::new(in_arr);

            let y = (x * b0) + z1;

            // NaN Hardening for SIMD path
            let finite_mask = y.is_finite_mask();
            let y = finite_mask.blend(y, FloatX16::splat(0.0));
            z1 = finite_mask.blend(((x * b1) - (y * a1)) + z2, FloatX16::splat(0.0));
            z2 = finite_mask.blend((x * b2) - (y * a2), FloatX16::splat(0.0));

            let out_arr: [f32; 16] = y.into();
            for ch in 0..16 { unsafe { *outputs[ch].add(i) = out_arr[ch] }; }
        }

        unsafe { store_f32x16_ptr(self.z1.as_mut_ptr(), z1) };
        unsafe { store_f32x16_ptr(self.z2.as_mut_ptr(), z2) };
    }

    pub fn process_4_channels(&mut self, inputs: [*const f32; 4], outputs: [*mut f32; 4], len: usize) {
        use wide::*;

        let b0 = f32x4::from(self.coeffs.b0);
        let b1 = f32x4::from(self.coeffs.b1);
        let b2 = f32x4::from(self.coeffs.b2);
        let a1 = f32x4::from(self.coeffs.a1);
        let a2 = f32x4::from(self.coeffs.a2);

        let mut z1 = f32x4::new([self.z1[0], self.z1[1], self.z1[2], self.z1[3]]);
        let mut z2 = f32x4::new([self.z2[0], self.z2[1], self.z2[2], self.z2[3]]);

        for i in 0..len {
            let x = unsafe {
                f32x4::new([
                    *inputs[0].add(i), *inputs[1].add(i), *inputs[2].add(i), *inputs[3].add(i)
                ])
            };

            let mut y = (x * b0) + z1;

            let y_arr: [f32; 4] = y.into();
            let mut finite = true;
            for val in &y_arr { if !val.is_finite() { finite = false; break; } }

            if !finite {
                y = f32x4::ZERO;
                z1 = f32x4::ZERO;
                z2 = f32x4::ZERO;
            } else {
                z1 = ((x * b1) - (y * a1)) + z2;
                z2 = (x * b2) - (y * a2);
            }

            let out_v: [f32; 4] = y.into();
            unsafe {
                *outputs[0].add(i) = out_v[0];
                *outputs[1].add(i) = out_v[1];
                *outputs[2].add(i) = out_v[2];
                *outputs[3].add(i) = out_v[3];
            }
        }

        let z1_arr: [f32; 4] = z1.into();
        self.z1[0..4].copy_from_slice(&z1_arr);
        let z2_arr: [f32; 4] = z2.into();
        self.z2[0..4].copy_from_slice(&z2_arr);
    }

    pub fn process_8_channels(&mut self, inputs: [*const f32; 8], outputs: [*mut f32; 8], len: usize) {
        use wide::*;

        let b0 = f32x8::from(self.coeffs.b0);
        let b1 = f32x8::from(self.coeffs.b1);
        let b2 = f32x8::from(self.coeffs.b2);
        let a1 = f32x8::from(self.coeffs.a1);
        let a2 = f32x8::from(self.coeffs.a2);

        let mut z1 = f32x8::new([
            self.z1[0], self.z1[1], self.z1[2], self.z1[3],
            self.z1[4], self.z1[5], self.z1[6], self.z1[7],
        ]);
        let mut z2 = f32x8::new([
            self.z2[0], self.z2[1], self.z2[2], self.z2[3],
            self.z2[4], self.z2[5], self.z2[6], self.z2[7],
        ]);

        for i in 0..len {
            // SAFETY: Caller ensures input pointers are valid for 'len' elements.
            let x = unsafe {
                f32x8::new([
                    *inputs[0].add(i), *inputs[1].add(i), *inputs[2].add(i), *inputs[3].add(i),
                    *inputs[4].add(i), *inputs[5].add(i), *inputs[6].add(i), *inputs[7].add(i)
                ])
            };

            let mut y = (x * b0) + z1;

            // NaN Hardening for 8-channel path
            // We use a manual check because f32x8 finite check is not built-in to 'wide' for all targets
            let y_arr: [f32; 8] = y.into();
            let mut finite = true;
            for val in &y_arr { if !val.is_finite() { finite = false; break; } }

            if !finite {
                y = f32x8::ZERO;
                z1 = f32x8::ZERO;
                z2 = f32x8::ZERO;
            } else {
                z1 = ((x * b1) - (y * a1)) + z2;
                z2 = (x * b2) - (y * a2);
            }

            let out_v: [f32; 8] = y.into();
            for (ch, &val) in out_v.iter().enumerate() {
                // SAFETY: Caller ensures output pointers are valid for 'len' elements.
                unsafe { *outputs.get_unchecked(ch).add(i) = val };
            }
        }

        let z1_arr: [f32; 8] = z1.into();
        self.z1[0..8].copy_from_slice(&z1_arr);
        let z2_arr: [f32; 8] = z2.into();
        self.z2[0..8].copy_from_slice(&z2_arr);
    }
}

impl crate::DspKernel for DjIsolator {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        self.process_block(inputs[0], outputs[0]);
    }

    fn reset(&mut self) {
        self.low_pass_1.reset();
        self.low_pass_2.reset();
        self.high_pass_1.reset();
        self.high_pass_2.reset();
        self.mid_low_hp_1.reset();
        self.mid_low_hp_2.reset();
        self.mid_high_lp_1.reset();
        self.mid_high_lp_2.reset();
    }

    fn set_parameter(&mut self, id: u32, value: f32, _ramp_samples: u32) {
        if !value.is_finite() { return; }
        if id < 3 {
            self.gains[id as usize] = value.clamp(0.0, 10.0);
        }
    }
}

/// A 3-band DJ Isolator (Kill EQ) using 4th-order Linkwitz-Riley crossovers.
/// Each crossover consists of two cascaded 2nd-order biquads for a 24dB/octave slope.
#[derive(Clone)]
pub struct DjIsolator {
    pub low_pass_1: BiquadFilter,
    pub low_pass_2: BiquadFilter,
    pub high_pass_1: BiquadFilter,
    pub high_pass_2: BiquadFilter,
    pub mid_low_hp_1: BiquadFilter,
    pub mid_low_hp_2: BiquadFilter,
    pub mid_high_lp_1: BiquadFilter,
    pub mid_high_lp_2: BiquadFilter,
    pub gains: [f32; 3], // Low, Mid, High gains (0.0 to 1.0+)
}

impl Default for DjIsolator {
    fn default() -> Self {
        Self::new()
    }
}

impl DjIsolator {
    pub fn new() -> Self {
        Self::with_sample_rate(44100.0)
    }

    pub fn with_sample_rate(sample_rate: f32) -> Self {
        let lp_coeffs = BiquadCoefficients::linkwitz_riley_lp(300.0, sample_rate);
        let hp_coeffs = BiquadCoefficients::linkwitz_riley_hp(3000.0, sample_rate);

        let mid_hp = BiquadCoefficients::linkwitz_riley_hp(300.0, sample_rate);
        let mid_lp = BiquadCoefficients::linkwitz_riley_lp(3000.0, sample_rate);

        Self {
            low_pass_1: BiquadFilter::new(lp_coeffs),
            low_pass_2: BiquadFilter::new(lp_coeffs),
            high_pass_1: BiquadFilter::new(hp_coeffs),
            high_pass_2: BiquadFilter::new(hp_coeffs),
            mid_low_hp_1: BiquadFilter::new(mid_hp),
            mid_low_hp_2: BiquadFilter::new(mid_hp),
            mid_high_lp_1: BiquadFilter::new(mid_lp),
            mid_high_lp_2: BiquadFilter::new(mid_lp),
            gains: [1.0, 1.0, 1.0],
        }
    }

    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        let g_l = self.gains[0];
        let g_m = self.gains[1];
        let g_h = self.gains[2];

        for i in 0..input.len() {
            let s = input[i];

            // 4th Order Low (2 cascaded LP)
            let l = self.low_pass_2.process_sample(self.low_pass_1.process_sample(s));

            // 4th Order High (2 cascaded HP)
            let h = self.high_pass_2.process_sample(self.high_pass_1.process_sample(s));

            // 4th Order Mid (HP 300 -> LP 3000)
            let m_low = self.mid_low_hp_2.process_sample(self.mid_low_hp_1.process_sample(s));
            let m = self.mid_high_lp_2.process_sample(self.mid_high_lp_1.process_sample(m_low));

            output[i] = l * g_l + m * g_m + h * g_h;
        }
    }

}

impl crate::DspKernel for BiquadFilter {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        self.process_block_unrolled(inputs[0], outputs[0]);
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

impl BiquadFilter {
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// A simple Envelope Follower using rectification and a one-pole low-pass filter.
#[derive(Debug, Clone, Copy)]
pub struct EnvelopeFollower {
    pub sample_rate: f32,
    pub attack_coeff: f32,
    pub release_coeff: f32,
    pub envelope: f32,
}

impl EnvelopeFollower {
    pub fn new(sample_rate: f32, attack_ms: f32, release_ms: f32) -> Self {
        Self {
            sample_rate,
            attack_coeff: (-1.0 / (sample_rate * attack_ms * 0.001)).exp(),
            release_coeff: (-1.0 / (sample_rate * release_ms * 0.001)).exp(),
            envelope: 0.0,
        }
    }

    pub fn set_times(&mut self, attack_ms: f32, release_ms: f32) {
        self.attack_coeff = (-1.0 / (self.sample_rate * attack_ms * 0.001)).exp();
        self.release_coeff = (-1.0 / (self.sample_rate * release_ms * 0.001)).exp();
    }

    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        self.process_with_sidechain(input, input, output);
    }

    pub fn process_with_sidechain(&mut self, input: &[f32], sidechain: &[f32], output: &mut [f32]) {
        let mut env = self.envelope;
        let attack = self.attack_coeff;
        let release = self.release_coeff;

        for i in 0..input.len().min(sidechain.len()) {
            let rect = sidechain[i].abs();
            if rect > env {
                env = rect + attack * (env - rect);
            } else {
                env = rect + release * (env - rect);
            }
            output[i] = env;
        }
        self.envelope = env;
    }
}

impl crate::DspKernel for EnvelopeFollower {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        self.process_block(inputs[0], outputs[0]);
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }

    fn set_parameter(&mut self, id: u32, value: f32, _ramp_samples: u32) {
        if id == 0 {
            self.attack_coeff = (-1.0 / (self.sample_rate * value.max(0.1) * 0.001)).exp();
        } else if id == 1 {
            self.release_coeff = (-1.0 / (self.sample_rate * value.max(0.1) * 0.001)).exp();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_biquad_unrolled_vs_sample() {
        let coeffs = BiquadCoefficients {
            b0: 0.1, b1: 0.2, b2: 0.3, a1: -0.5, a2: 0.2
        };
        let mut filter_scalar = BiquadFilter::new(coeffs);
        let mut filter_unrolled = BiquadFilter::new(coeffs);

        let input = vec![0.5; 100];
        let mut out_scalar = vec![0.0; 100];
        let mut out_unrolled = vec![0.0; 100];

        for i in 0..100 {
            out_scalar[i] = filter_scalar.process_sample(input[i]);
        }
        filter_unrolled.process_block_unrolled(&input, &mut out_unrolled);

        for i in 0..100 {
            assert!((out_scalar[i] - out_unrolled[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_simd_biquad_8_channels() {
        let coeffs = BiquadCoefficients {
            b0: 0.1, b1: 0.2, b2: 0.3, a1: -0.5, a2: 0.2
        };
        let mut simd_filter = SimdBiquad::new(coeffs);
        let mut scalar_filters: Vec<BiquadFilter> = (0..8).map(|_| BiquadFilter::new(coeffs)).collect();

        let len = 64;
        let mut inputs = vec![vec![0.0f32; len]; 8];
        let mut outputs_simd = vec![vec![0.0f32; len]; 8];
        let mut outputs_scalar = vec![vec![0.0f32; len]; 8];

        for ch in 0..8 {
            for i in 0..len { inputs[ch][i] = (ch + i) as f32 * 0.01; }
        }

        let in_ptrs: [*const f32; 8] = [
            inputs[0].as_ptr(), inputs[1].as_ptr(), inputs[2].as_ptr(), inputs[3].as_ptr(),
            inputs[4].as_ptr(), inputs[5].as_ptr(), inputs[6].as_ptr(), inputs[7].as_ptr(),
        ];
        let out_ptrs: [*mut f32; 8] = [
            outputs_simd[0].as_mut_ptr(), outputs_simd[1].as_mut_ptr(), outputs_simd[2].as_mut_ptr(), outputs_simd[3].as_mut_ptr(),
            outputs_simd[4].as_mut_ptr(), outputs_simd[5].as_mut_ptr(), outputs_simd[6].as_mut_ptr(), outputs_simd[7].as_mut_ptr(),
        ];

        simd_filter.process_8_channels(in_ptrs, out_ptrs, len);

        for ch in 0..8 {
            scalar_filters[ch].process_block_unrolled(&inputs[ch], &mut outputs_scalar[ch]);
            for i in 0..len {
                assert!((outputs_simd[ch][i] - outputs_scalar[ch][i]).abs() < 1e-6);
            }
        }
    }

    proptest! {
        #[test]
        fn test_biquad_stability(
            b0 in -1.0f32..1.0f32,
            b1 in -1.0f32..1.0f32,
            b2 in -1.0f32..1.0f32,
            a1 in -0.5f32..0.5f32,
            a2 in -0.2f32..0.2f32,
        ) {
            let coeffs = BiquadCoefficients { b0, b1, b2, a1, a2 };
            let mut filter = BiquadFilter::new(coeffs);
            let input = vec![1.0; 100];
            let mut output = vec![0.0; 100];
            filter.process_block_unrolled(&input, &mut output);
            for &sample in &output {
                prop_assert!(sample.is_finite());
            }
        }
    }
}
