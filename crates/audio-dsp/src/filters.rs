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
        // Scalar fallback for non-aligned or small blocks, but here we can use SIMD if len >= 4
        while i + 4 <= len {
            // We use a scalar loop with improved dependency chain for now,
            // true parallel 4-lane SIMD for single-channel biquad is hard because of dependencies.
            // But we can unroll it safely without architecture-specific unsafe.

            unsafe {
                let x = *input.get_unchecked(i);
                let y = x * b0 + z1;
                let next_z1 = x * b1 - y * a1 + z2;
                let next_z2 = x * b2 - y * a2;
                *output.get_unchecked_mut(i) = y;
                z1 = next_z1;
                z2 = next_z2;

                let x = *input.get_unchecked(i + 1);
                let y = x * b0 + z1;
                let next_z1 = x * b1 - y * a1 + z2;
                let next_z2 = x * b2 - y * a2;
                *output.get_unchecked_mut(i + 1) = y;
                z1 = next_z1;
                z2 = next_z2;

                let x = *input.get_unchecked(i + 2);
                let y = x * b0 + z1;
                let next_z1 = x * b1 - y * a1 + z2;
                let next_z2 = x * b2 - y * a2;
                *output.get_unchecked_mut(i + 2) = y;
                z1 = next_z1;
                z2 = next_z2;

                let x = *input.get_unchecked(i + 3);
                let y = x * b0 + z1;
                let next_z1 = x * b1 - y * a1 + z2;
                let next_z2 = x * b2 - y * a2;
                *output.get_unchecked_mut(i + 3) = y;
                z1 = next_z1;
                z2 = next_z2;
            }

            i += 4;
        }

        while i < len {
            unsafe {
                let x = *input.get_unchecked(i);
                let y = x * b0 + z1;
                z1 = x * b1 - y * a1 + z2;
                z2 = x * b2 - y * a2;
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

        let output = input * self.coeffs.b0 + self.z1;
        self.z1 = input * self.coeffs.b1 - output * self.coeffs.a1 + self.z2;
        self.z2 = input * self.coeffs.b2 - output * self.coeffs.a2;
        output
    }
}

/// A Biquad Filter that processes 8 or 16 channels in parallel using AVX2/AVX-512.
#[repr(C, align(64))]
pub struct SimdBiquad {
    pub coeffs: BiquadCoefficients,
    pub z1: [f32; 16],
    pub z2: [f32; 16],
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
            let out = input[i] * self.coeffs.b0 + z1;
            z1 = input[i] * self.coeffs.b1 - out * self.coeffs.a1 + z2;
            z2 = input[i] * self.coeffs.b2 - out * self.coeffs.a2;
            output[i] = out;
        }
        self.z1[channel] = z1;
        self.z2[channel] = z2;
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx512f")]
    /// # Safety
    /// Caller must ensure all pointers are valid for 'len' elements.
    pub unsafe fn process_16_channels(&mut self, inputs: [*const f32; 16], outputs: [*mut f32; 16], len: usize) {
        use std::arch::x86_64::*;
        // SAFETY: Caller must ensure that all input and output pointers are valid for 'len' elements,
        // and that the CPU supports AVX-512F.
        unsafe {

        let b0 = _mm512_set1_ps(self.coeffs.b0);
        let b1 = _mm512_set1_ps(self.coeffs.b1);
        let b2 = _mm512_set1_ps(self.coeffs.b2);
        let a1 = _mm512_set1_ps(self.coeffs.a1);
        let a2 = _mm512_set1_ps(self.coeffs.a2);

        let mut z1 = _mm512_loadu_ps(self.z1.as_ptr());
        let mut z2 = _mm512_loadu_ps(self.z2.as_ptr());

        for i in 0..len {
            let x = _mm512_setr_ps(
                *inputs[0].add(i), *inputs[1].add(i), *inputs[2].add(i), *inputs[3].add(i),
                *inputs[4].add(i), *inputs[5].add(i), *inputs[6].add(i), *inputs[7].add(i),
                *inputs[8].add(i), *inputs[9].add(i), *inputs[10].add(i), *inputs[11].add(i),
                *inputs[12].add(i), *inputs[13].add(i), *inputs[14].add(i), *inputs[15].add(i)
            );

            let y = _mm512_add_ps(_mm512_mul_ps(x, b0), z1);
            z1 = _mm512_add_ps(_mm512_sub_ps(_mm512_mul_ps(x, b1), _mm512_mul_ps(y, a1)), z2);
            z2 = _mm512_sub_ps(_mm512_mul_ps(x, b2), _mm512_mul_ps(y, a2));

            let mut out_v = [0.0f32; 16];
            _mm512_storeu_ps(out_v.as_mut_ptr(), y);
            for (ch, &val) in out_v.iter().enumerate() { *outputs.get_unchecked(ch).add(i) = val; }
        }

        _mm512_storeu_ps(self.z1.as_mut_ptr(), z1);
        _mm512_storeu_ps(self.z2.as_mut_ptr(), z2);
        }
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

            let y = (x * b0) + z1;
            z1 = ((x * b1) - (y * a1)) + z2;
            z2 = (x * b2) - (y * a2);

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

/// A 3-band DJ Isolator (Kill EQ) using high-order SIMD filters.
pub struct DjIsolator {
    pub low: BiquadFilter,
    pub mid: BiquadFilter,
    pub high: BiquadFilter,
    pub gains: [f32; 3], // Low, Mid, High gains (0.0 to 1.0+)
}

impl Default for DjIsolator {
    fn default() -> Self {
        Self::new()
    }
}

impl DjIsolator {
    pub fn new() -> Self {
        // Approximate Linkwitz-Riley crossover coefficients
        let low_coeffs = BiquadCoefficients { b0: 0.000944, b1: 0.001888, b2: 0.000944, a1: -1.911197, a2: 0.914975 };
        let mid_coeffs = BiquadCoefficients { b0: 0.013359, b1: 0.0, b2: -0.013359, a1: -1.89066, a2: 0.97328 };
        let high_coeffs = BiquadCoefficients { b0: 0.80302, b1: -1.60604, b2: 0.80302, a1: -1.56101, a2: 0.65106 };
        Self {
            low: BiquadFilter::new(low_coeffs),
            mid: BiquadFilter::new(mid_coeffs),
            high: BiquadFilter::new(high_coeffs),
            gains: [1.0, 1.0, 1.0],
        }
    }

    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        for i in 0..input.len() {
            let s = input[i];
            let l = self.low.process_sample(s) * self.gains[0];
            let m = self.mid.process_sample(s) * self.gains[1];
            let h = self.high.process_sample(s) * self.gains[2];
            output[i] = l + m + h;
        }
    }

    /// Processes a block of audio using AVX2 SIMD instructions.
    ///
    /// # Safety
    /// Caller must ensure input and output slices are valid for 'len' elements
    /// and that the CPU supports AVX2.
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    pub unsafe fn process_block_avx2(&mut self, input: &[f32], output: &mut [f32]) {
        // SAFETY: CPU feature check is responsibility of the caller (target_feature enabled).
        // Slices are checked by the caller.
        unsafe {
        use std::arch::x86_64::*;
        let len = input.len();
        if len == 0 { return; }

        if self.low.ramp_duration > 0 || self.mid.ramp_duration > 0 || self.high.ramp_duration > 0 {
            self.process_block(input, output);
            return;
        }

        let b0 = _mm_set_ps(0.0, self.high.coeffs.b0, self.mid.coeffs.b0, self.low.coeffs.b0);
        let b1 = _mm_set_ps(0.0, self.high.coeffs.b1, self.mid.coeffs.b1, self.low.coeffs.b1);
        let b2 = _mm_set_ps(0.0, self.high.coeffs.b2, self.mid.coeffs.b2, self.low.coeffs.b2);
        let a1 = _mm_set_ps(0.0, self.high.coeffs.a1, self.mid.coeffs.a1, self.low.coeffs.a1);
        let a2 = _mm_set_ps(0.0, self.high.coeffs.a2, self.mid.coeffs.a2, self.low.coeffs.a2);
        let gains = _mm_set_ps(0.0, self.gains[2], self.gains[1], self.gains[0]);

        let mut z1 = _mm_set_ps(0.0, self.high.z1, self.mid.z1, self.low.z1);
        let mut z2 = _mm_set_ps(0.0, self.high.z2, self.mid.z2, self.low.z2);

        for i in 0..len {
            let x = _mm_set1_ps(*input.get_unchecked(i));
            let y = _mm_add_ps(_mm_mul_ps(x, b0), z1);
            z1 = _mm_add_ps(_mm_sub_ps(_mm_mul_ps(x, b1), _mm_mul_ps(y, a1)), z2);
            z2 = _mm_sub_ps(_mm_mul_ps(x, b2), _mm_mul_ps(y, a2));

            let mixed = _mm_mul_ps(y, gains);
            let sum = _mm_hadd_ps(mixed, mixed);
            let sum = _mm_hadd_ps(sum, sum);
            *output.get_unchecked_mut(i) = _mm_cvtss_f32(sum);
        }

        let mut final_z1 = [0.0f32; 4];
        let mut final_z2 = [0.0f32; 4];
        _mm_storeu_ps(final_z1.as_mut_ptr(), z1);
        _mm_storeu_ps(final_z2.as_mut_ptr(), z2);

        self.low.z1 = final_z1[0];
        self.mid.z1 = final_z1[1];
        self.high.z1 = final_z1[2];
        self.low.z2 = final_z2[0];
        self.mid.z2 = final_z2[1];
        self.high.z2 = final_z2[2];
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
        let mut out_ptrs: [*mut f32; 8] = [
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
