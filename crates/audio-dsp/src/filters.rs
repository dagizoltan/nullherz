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

    /// Optimized block processing using Direct Form II Transposed.
    /// Uses a manually unrolled loop to maximize throughput and minimize dependency stalls.
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "sse3")]
    /// # Safety
    /// Caller must ensure input and output are valid for 'len' elements.
    pub unsafe fn process_block_simd(&mut self, input: &[f32], output: &mut [f32]) {
        // SAFETY: The requirements are outlined in the doc comment.
        unsafe {
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
        while i + 4 <= len {
            let x0 = *input.get_unchecked(i);
            let y0 = x0 * b0 + z1;
            let z1_0 = x0 * b1 - y0 * a1 + z2;
            let z2_0 = x0 * b2 - y0 * a2;
            *output.get_unchecked_mut(i) = y0;

            let x1 = *input.get_unchecked(i + 1);
            let y1 = x1 * b0 + z1_0;
            let z1_1 = x1 * b1 - y1 * a1 + z2_0;
            let z2_1 = x1 * b2 - y1 * a2;
            *output.get_unchecked_mut(i + 1) = y1;

            let x2 = *input.get_unchecked(i + 2);
            let y2 = x2 * b0 + z1_1;
            let z1_2 = x2 * b1 - y2 * a1 + z2_1;
            let z2_2 = x2 * b2 - y2 * a2;
            *output.get_unchecked_mut(i + 2) = y2;

            let x3 = *input.get_unchecked(i + 3);
            let y3 = x3 * b0 + z1_2;
            z1 = x3 * b1 - y3 * a1 + z2_2;
            z2 = x3 * b2 - y3 * a2;
            *output.get_unchecked_mut(i + 3) = y3;

            i += 4;
        }

        while i < len {
            let x = *input.get_unchecked(i);
            let y = x * b0 + z1;
            z1 = x * b1 - y * a1 + z2;
            z2 = x * b2 - y * a2;
            *output.get_unchecked_mut(i) = y;
            i += 1;
        }

        self.z1 = z1;
        self.z2 = z2;
        }
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
    pub(crate) z1: [f32; 16],
    pub(crate) z2: [f32; 16],
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

    #[cfg(target_arch = "aarch64")]
    pub unsafe fn process_block_simd(&mut self, input: &[f32], output: &mut [f32]) {
        unsafe {
        use std::arch::aarch64::*;
        let len = input.len();
        if len == 0 { return; }

        let mut z1 = self.z1;
        let mut z2 = self.z2;
        let b0 = self.coeffs.b0;
        let b1 = self.coeffs.b1;
        let b2 = self.coeffs.b2;
        let a1 = self.coeffs.a1;
        let a2 = self.coeffs.a2;

        for i in 0..len {
            let x = *input.get_unchecked(i);
            let y = x * b0 + z1;
            z1 = x * b1 - y * a1 + z2;
            z2 = x * b2 - y * a2;
            *output.get_unchecked_mut(i) = y;
        }
        // This ARM implementation needs fixing to use self.z1/z2 properly like x86
        // but for now we maintain the previous logic structure.
        }
    }

    #[cfg(target_arch = "aarch64")]
    pub unsafe fn process_8_channels(&mut self, inputs: [*const f32; 8], outputs: [*mut f32; 8], len: usize) {
        use std::arch::aarch64::*;
        // SAFETY: Caller ensures input/output pointers are valid for 'len' elements and 8 channels.
        unsafe {

        let b0 = vdupq_n_f32(self.coeffs.b0);
        let b1 = vdupq_n_f32(self.coeffs.b1);
        let b2 = vdupq_n_f32(self.coeffs.b2);
        let a1 = vdupq_n_f32(self.coeffs.a1);
        let a2 = vdupq_n_f32(self.coeffs.a2);

        let mut z1_0 = vld1q_f32(self.z1.as_ptr());
        let mut z1_1 = vld1q_f32(self.z1.as_ptr().add(4));
        let mut z2_0 = vld1q_f32(self.z2.as_ptr());
        let mut z2_1 = vld1q_f32(self.z2.as_ptr().add(4));

        for i in 0..len {
            let x0 = unsafe { vsetq_lane_f32(*inputs[0].add(i), vdupq_n_f32(0.0), 0) };
            let x0 = unsafe { vsetq_lane_f32(*inputs[1].add(i), x0, 1) };
            let x0 = unsafe { vsetq_lane_f32(*inputs[2].add(i), x0, 2) };
            let x0 = unsafe { vsetq_lane_f32(*inputs[3].add(i), x0, 3) };

            let x1 = unsafe { vsetq_lane_f32(*inputs[4].add(i), vdupq_n_f32(0.0), 0) };
            let x1 = unsafe { vsetq_lane_f32(*inputs[5].add(i), x1, 1) };
            let x1 = unsafe { vsetq_lane_f32(*inputs[6].add(i), x1, 2) };
            let x1 = unsafe { vsetq_lane_f32(*inputs[7].add(i), x1, 3) };

            // Group 0 (Ch 0-3)
            let y0 = vaddq_f32(vmulq_f32(x0, b0), z1_0);
            z1_0 = vaddq_f32(vsubq_f32(vmulq_f32(x0, b1), vmulq_f32(y0, a1)), z2_0);
            z2_0 = vsubq_f32(vmulq_f32(x0, b2), vmulq_f32(y0, a2));

            // Group 1 (Ch 4-7)
            let y1 = vaddq_f32(vmulq_f32(x1, b0), z1_1);
            z1_1 = vaddq_f32(vsubq_f32(vmulq_f32(x1, b1), vmulq_f32(y1, a1)), z2_1);
            z2_1 = vsubq_f32(vmulq_f32(x1, b2), vmulq_f32(y1, a2));

            let mut out0 = [0.0f32; 4];
            let mut out1 = [0.0f32; 4];
            vst1q_f32(out0.as_mut_ptr(), y0);
            vst1q_f32(out1.as_mut_ptr(), y1);

            for ch in 0..4 { unsafe { *outputs.get_unchecked(ch).add(i) = out0[ch] } ; }
            for ch in 0..4 { unsafe { *outputs.get_unchecked(ch+4).add(i) = out1[ch] } ; }
        }

        vst1q_f32(self.z1.as_mut_ptr(), z1_0);
        vst1q_f32(self.z1.as_mut_ptr().add(4), z1_1);
        vst1q_f32(self.z2.as_mut_ptr(), z2_0);
        vst1q_f32(self.z2.as_mut_ptr().add(4), z2_1);
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx512f")]
    /// # Safety
    /// Caller must ensure all pointers are valid for 'len' elements.
    pub unsafe fn process_16_channels(&mut self, inputs: [*const f32; 16], outputs: [*mut f32; 16], len: usize) {
        use std::arch::x86_64::*;
        // SAFETY: The requirements are outlined in the doc comment.
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

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    /// # Safety
    /// Caller must ensure all pointers are valid for 'len' elements.
    pub unsafe fn process_8_channels(&mut self, inputs: [*const f32; 8], outputs: [*mut f32; 8], len: usize) {
        use std::arch::x86_64::*;
        // SAFETY: The requirements are outlined in the doc comment.
        unsafe {

            let b0 = _mm256_set1_ps(self.coeffs.b0);
            let b1 = _mm256_set1_ps(self.coeffs.b1);
            let b2 = _mm256_set1_ps(self.coeffs.b2);
            let a1 = _mm256_set1_ps(self.coeffs.a1);
            let a2 = _mm256_set1_ps(self.coeffs.a2);

            let mut z1 = _mm256_loadu_ps(self.z1.as_ptr());
            let mut z2 = _mm256_loadu_ps(self.z2.as_ptr());

            for i in 0..len {
                let x = _mm256_setr_ps(
                    *inputs[0].add(i), *inputs[1].add(i), *inputs[2].add(i), *inputs[3].add(i),
                    *inputs[4].add(i), *inputs[5].add(i), *inputs[6].add(i), *inputs[7].add(i)
                );

                let y = _mm256_add_ps(_mm256_mul_ps(x, b0), z1);
                z1 = _mm256_add_ps(_mm256_sub_ps(_mm256_mul_ps(x, b1), _mm256_mul_ps(y, a1)), z2);
                z2 = _mm256_sub_ps(_mm256_mul_ps(x, b2), _mm256_mul_ps(y, a2));

                let mut out_v = [0.0f32; 8];
                _mm256_storeu_ps(out_v.as_mut_ptr(), y);
                for (ch, &val) in out_v.iter().enumerate() { *outputs.get_unchecked(ch).add(i) = val; }
            }

            _mm256_storeu_ps(self.z1.as_mut_ptr(), z1);
            _mm256_storeu_ps(self.z2.as_mut_ptr(), z2);
        }
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

    /// # Safety
    /// Caller must ensure input and output are valid for 'len' elements.
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    pub unsafe fn process_block_avx2(&mut self, input: &[f32], output: &mut [f32]) {
        // SAFETY: The requirements are outlined in the doc comment.
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
