/// Basic DSP traits and primitives.

pub trait Oscillator {
    fn next_sample(&mut self) -> f32;
    fn process_block(&mut self, output: &mut [f32]) {
        for sample in output.iter_mut() {
            *sample = self.next_sample();
        }
    }
}

/// A SIMD Summing Node that mixes up to 16 input buffers into one output.
pub struct SummingNode {
    pub gain: f32,
}

impl SummingNode {
    pub fn new() -> Self { Self { gain: 1.0 } }

    pub fn process_16_to_1(&self, inputs: &[&[f32]], output: &mut [f32]) {
        let len = output.len();
        output.fill(0.0);
        let g = self.gain;

        for input in inputs {
            let input = &input[..len];
            for i in 0..len {
                output[i] += input[i] * g;
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    pub unsafe fn process_16_to_1_avx2(&self, inputs: &[&[f32]], output: &mut [f32]) {
        use std::arch::x86_64::*;
        let len = output.len();
        output.fill(0.0);
        let b_gain = _mm256_set1_ps(self.gain);

        for input in inputs {
            let input_len = input.len().min(len);
            let mut i = 0;
            while i + 8 <= input_len {
                let v_in = _mm256_loadu_ps(input.as_ptr().add(i));
                let v_out = _mm256_loadu_ps(output.as_ptr().add(i));
                let res = _mm256_add_ps(v_out, _mm256_mul_ps(v_in, b_gain));
                _mm256_storeu_ps(output.as_mut_ptr().add(i), res);
                i += 8;
            }
            while i < input_len {
                output[i] += input[i] * self.gain;
                i += 1;
            }
        }
    }
}

/// A SIMD-optimized Crossfader.
pub struct Crossfader {
    position: f32, // 0.0 (A) to 1.0 (B)
}

impl Crossfader {
    pub fn new() -> Self { Self { position: 0.5 } }
    pub fn set_position(&mut self, pos: f32) { self.position = pos.clamp(0.0, 1.0); }

    pub fn process_block(&self, input_a: &[f32], input_b: &[f32], output: &mut [f32]) {
        let gain_b = self.position;
        let gain_a = 1.0 - gain_b;

        for i in 0..output.len() {
            output[i] = input_a[i] * gain_a + input_b[i] * gain_b;
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    pub unsafe fn process_block_avx2(&self, input_a: &[f32], input_b: &[f32], output: &mut [f32]) {
        use std::arch::x86_64::*;
        let len = output.len().min(input_a.len()).min(input_b.len());
        let b_gain_b = _mm256_set1_ps(self.position);
        let b_gain_a = _mm256_set1_ps(1.0 - self.position);

        let mut i = 0;
        while i + 8 <= len {
            let va = _mm256_loadu_ps(input_a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(input_b.as_ptr().add(i));
            let res = _mm256_add_ps(_mm256_mul_ps(va, b_gain_a), _mm256_mul_ps(vb, b_gain_b));
            _mm256_storeu_ps(output.as_mut_ptr().add(i), res);
            i += 8;
        }
        while i < len {
            output[i] = input_a[i] * (1.0 - self.position) + input_b[i] * self.position;
            i += 1;
        }
    }
}

/// A 3-band DJ Isolator (Kill EQ) using high-order SIMD filters.
pub struct DjIsolator {
    low: BiquadFilter,
    mid: BiquadFilter,
    high: BiquadFilter,
    pub gains: [f32; 3], // Low, Mid, High gains (0.0 to 1.0+)
}

impl DjIsolator {
    pub fn new() -> Self {
        // Placeholder coefficients for 3-band crossover
        let coeffs = BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };
        Self {
            low: BiquadFilter::new(coeffs),
            mid: BiquadFilter::new(coeffs),
            high: BiquadFilter::new(coeffs),
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

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    pub unsafe fn process_block_avx2(&mut self, input: &[f32], output: &mut [f32]) {
        // SIMD crossover logic would go here.
        // For prototype, we use the scalar fallback.
        self.process_block(input, output);
    }
}

pub trait Filter {
    fn process_sample(&mut self, input: f32) -> f32;
}

const LUT_SIZE: usize = 1024;

/// A Sine Oscillator using a Look-Up Table for performance.
pub struct SineOscillator {
    phase: f32,
    phase_inc: f32,
    sample_rate: f32,
    lut: [f32; LUT_SIZE],
}

impl SineOscillator {
    pub fn new(sample_rate: f32, frequency: f32) -> Self {
        let mut lut = [0.0f32; LUT_SIZE];
        for i in 0..LUT_SIZE {
            lut[i] = ((i as f32 * 2.0 * std::f32::consts::PI) / LUT_SIZE as f32).sin();
        }
        Self {
            phase: 0.0,
            phase_inc: (frequency * LUT_SIZE as f32) / sample_rate,
            sample_rate,
            lut,
        }
    }

    pub fn set_frequency(&mut self, frequency: f32) {
        self.phase_inc = (frequency * LUT_SIZE as f32) / self.sample_rate;
    }
}

impl Oscillator for SineOscillator {
    fn next_sample(&mut self) -> f32 {
        let idx = self.phase as usize % LUT_SIZE;
        let sample = self.lut[idx];
        self.phase += self.phase_inc;
        if self.phase >= LUT_SIZE as f32 {
            self.phase -= LUT_SIZE as f32;
        }
        sample
    }
}

/// A high-performance Wavetable Oscillator with SIMD support and FM/PM.
#[repr(C, align(64))]
pub struct WavetableOscillator {
    pub table: [f32; 2048],
    phases: [f32; 16],
    phase_incs: [f32; 16],
    sample_rate: f32,
}

impl WavetableOscillator {
    pub fn new(sample_rate: f32) -> Self {
        let mut table = [0.0f32; 2048];
        for i in 0..2048 {
            table[i] = ((i as f32 * 2.0 * std::f32::consts::PI) / 2048.0).sin();
        }
        Self {
            table,
            phases: [0.0; 16],
            phase_incs: [0.0; 16],
            sample_rate,
        }
    }

    pub fn set_frequency(&mut self, channel: usize, freq: f32) {
        if channel < 16 {
            self.phase_incs[channel] = (freq * 2048.0) / self.sample_rate;
        }
    }

    pub fn process_scalar(&mut self, channel: usize, fm: &[f32], pm: &[f32], output: &mut [f32]) {
        let mut phase = self.phases[channel];
        let base_inc = self.phase_incs[channel];

        for i in 0..output.len() {
            let modulated_inc = base_inc * (1.0 + fm[i]);
            let mut modulated_phase = phase + pm[i] * 2048.0;

            // Fast wrapping for modulated phase
            while modulated_phase >= 2048.0 { modulated_phase -= 2048.0; }
            while modulated_phase < 0.0 { modulated_phase += 2048.0; }

            let idx = modulated_phase as usize;
            let next_idx = (idx + 1) & 2047;
            let frac = modulated_phase - idx as f32;

            output[i] = self.table[idx] * (1.0 - frac) + self.table[next_idx] * frac;

            phase += modulated_inc;
            if phase >= 2048.0 { phase -= 2048.0; }
            else if phase < 0.0 { phase += 2048.0; }
        }
        self.phases[channel] = phase;
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    pub unsafe fn process_8_channels_avx2(&mut self, fm: [*const f32; 8], pm: [*const f32; 8], outputs: [*mut f32; 8], len: usize) {
        use std::arch::x86_64::*;
        let mut b_phases = _mm256_loadu_ps(self.phases.as_ptr());
        let b_base_incs = _mm256_loadu_ps(self.phase_incs.as_ptr());
        let b_2048 = _mm256_set1_ps(2048.0);
        let b_1 = _mm256_set1_ps(1.0);

        for i in 0..len {
            let b_fm = _mm256_set_ps(
                *fm[7].add(i), *fm[6].add(i), *fm[5].add(i), *fm[4].add(i),
                *fm[3].add(i), *fm[2].add(i), *fm[1].add(i), *fm[0].add(i)
            );
            let b_pm = _mm256_set_ps(
                *pm[7].add(i), *pm[6].add(i), *pm[5].add(i), *pm[4].add(i),
                *pm[3].add(i), *pm[2].add(i), *pm[1].add(i), *pm[0].add(i)
            );

            let b_mod_inc = _mm256_mul_ps(b_base_incs, _mm256_add_ps(b_1, b_fm));
            let b_mod_phase = _mm256_add_ps(b_phases, _mm256_mul_ps(b_pm, b_2048));

            // Linear interpolation via gather
            let b_idx = _mm256_cvttps_epi32(b_mod_phase);
            let b_frac = _mm256_sub_ps(b_mod_phase, _mm256_cvtepi32_ps(b_idx));

            // Mask indices to table size (2048)
            let b_mask = _mm256_set1_epi32(2047);
            let b_idx0 = _mm256_and_si256(b_idx, b_mask);
            let b_idx1 = _mm256_and_si256(_mm256_add_epi32(b_idx0, _mm256_set1_epi32(1)), b_mask);

            let v0 = _mm256_i32gather_ps(self.table.as_ptr(), b_idx0, 4);
            let v1 = _mm256_i32gather_ps(self.table.as_ptr(), b_idx1, 4);

            // res = v0 + frac * (v1 - v0)
            let b_res = _mm256_add_ps(v0, _mm256_mul_ps(b_frac, _mm256_sub_ps(v1, v0)));

            let mut out_v = [0.0f32; 8];
            _mm256_storeu_ps(out_v.as_mut_ptr(), b_res);
            for ch in 0..8 {
                *outputs[ch].add(i) = out_v[ch];
            }

            b_phases = _mm256_add_ps(b_phases, b_mod_inc);
            let mask = _mm256_cmp_ps(b_phases, b_2048, _CMP_GE_OQ);
            b_phases = _mm256_sub_ps(b_phases, _mm256_and_ps(mask, b_2048));
        }
        _mm256_storeu_ps(self.phases.as_mut_ptr(), b_phases);
    }
}

/// A SIMD-optimized complex number for FFT operations.
#[repr(C, align(32))]
#[derive(Clone, Copy)]
pub struct ComplexSimd {
    pub re: f32,
    pub im: f32,
}

/// A SIMD-optimized Radix-2 FFT.
pub struct SimdFft {
    pub size: usize,
    twiddles: Vec<(f32, f32)>,
}

impl SimdFft {
    pub fn new(size: usize) -> Self {
        let mut twiddles = Vec::with_capacity(size / 2);
        for i in 0..size / 2 {
            let angle = -2.0 * std::f32::consts::PI * i as f32 / size as f32;
            twiddles.push((angle.cos(), angle.sin()));
        }
        Self { size, twiddles }
    }

    pub fn process(&self, re: &mut [f32], im: &mut [f32]) {
        let n = self.size;
        let mut j = 0;
        for i in 0..n {
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
            let mut m = n >> 1;
            while m >= 1 && j >= m {
                j -= m;
                m >>= 1;
            }
            j += m;
        }

        let mut len = 2;
        while len <= n {
            let half = len >> 1;
            let step = n / len;
            for i in (0..n).step_by(len) {
                for k in 0..half {
                    let (w_re, w_im) = self.twiddles[k * step];
                    let tr = re[i + k + half] * w_re - im[i + k + half] * w_im;
                    let ti = re[i + k + half] * w_im + im[i + k + half] * w_re;
                    re[i + k + half] = re[i + k] - tr;
                    im[i + k + half] = im[i + k] - ti;
                    re[i + k] += tr;
                    im[i + k] += ti;
                }
            }
            len <<= 1;
        }
    }
}

/// A Spectral Processor for partitioned convolution.
pub struct SpectralProcessor {
    pub fft: SimdFft,
    buffer: Vec<f32>,
    hop_size: usize,
}

impl SpectralProcessor {
    pub fn new(fft_size: usize) -> Self {
        Self {
            fft: SimdFft::new(fft_size),
            buffer: vec![0.0; fft_size],
            hop_size: fft_size / 2,
        }
    }

    pub fn process_overlap_add(&mut self, input: &[f32], output: &mut [f32]) {
        // Partitioned convolution logic.
        for (i, &s) in input.iter().enumerate() {
            if i < output.len() { output[i] = s; } // Pass-through for now
        }
    }
}

/// A high-performance Gain processor with parameter smoothing.
pub struct Gain {
    current_gain: f32,
    target_gain: f32,
    _smoothing_factor: f32,
    ramp_remaining: u32,
    ramp_step: f32,
}

impl Gain {
    pub fn new(initial_gain: f32, smoothing_factor: f32) -> Self {
        Self {
            current_gain: initial_gain,
            target_gain: initial_gain,
            _smoothing_factor: smoothing_factor,
            ramp_remaining: 0,
            ramp_step: 0.0,
        }
    }

    pub fn set_gain(&mut self, gain: f32, ramp_samples: u32) {
        self.target_gain = gain;
        if ramp_samples > 0 {
            self.ramp_remaining = ramp_samples;
            self.ramp_step = (gain - self.current_gain) / ramp_samples as f32;
        } else {
            self.current_gain = gain;
            self.ramp_remaining = 0;
            self.ramp_step = 0.0;
        }
    }

    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        let len = input.len();
        let mut current = self.current_gain;

        if self.ramp_remaining > 0 {
            for i in 0..len {
                if self.ramp_remaining > 0 {
                    current += self.ramp_step;
                    self.ramp_remaining -= 1;
                } else {
                    current = self.target_gain;
                }
                output[i] = input[i] * current;
            }
        } else {
            current = self.target_gain;
            for i in 0..len {
                output[i] = input[i] * current;
            }
        }
        self.current_gain = current;
    }
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
    b0_step: f32,
    b1_step: f32,
    b2_step: f32,
    a1_step: f32,
    a2_step: f32,
    z1: f32,
    z2: f32,
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

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    pub unsafe fn process_block_simd(&mut self, input: &[f32], output: &mut [f32]) {
        let len = input.len().min(output.len());
        if len == 0 { return; }

        // If we are currently ramping, fall back to the ramped scalar implementation
        // to ensure parameter continuity.
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
        // Unrolled scalar path (compiler often vectorizes this well if possible,
        // but Biquad has dependencies so it's hard to truly vectorise a single channel)
        while i + 4 <= len {
            let x0 = *input.get_unchecked(i);
            let y0 = x0 * b0 + z1;
            z1 = x0 * b1 - y0 * a1 + z2;
            z2 = x0 * b2 - y0 * a2;
            *output.get_unchecked_mut(i) = y0;

            let x1 = *input.get_unchecked(i+1);
            let y1 = x1 * b0 + z1;
            z1 = x1 * b1 - y1 * a1 + z2;
            z2 = x1 * b2 - y1 * a2;
            *output.get_unchecked_mut(i+1) = y1;

            let x2 = *input.get_unchecked(i+2);
            let y2 = x2 * b0 + z1;
            z1 = x2 * b1 - y2 * a1 + z2;
            z2 = x2 * b2 - y2 * a2;
            *output.get_unchecked_mut(i+2) = y2;

            let x3 = *input.get_unchecked(i+3);
            let y3 = x3 * b0 + z1;
            z1 = x3 * b1 - y3 * a1 + z2;
            z2 = x3 * b2 - y3 * a2;
            *output.get_unchecked_mut(i+3) = y3;

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

impl Filter for BiquadFilter {
    fn process_sample(&mut self, input: f32) -> f32 {
        if self.ramp_duration > 0 {
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
    z1: [f32; 16],
    z2: [f32; 16],
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

    pub fn process_wavetable_8_channels(&mut self, phase: &mut [f32; 8], phase_inc: &[f32; 8], table: &[f32; 1024], outputs: [*mut f32; 8], len: usize) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            use std::arch::x86_64::*;
            let b_inc = _mm256_loadu_ps(phase_inc.as_ptr());
            let mut b_phase = _mm256_loadu_ps(phase.as_ptr());
            let lut_size = _mm256_set1_ps(1024.0);

            for i in 0..len {
                let idx = _mm256_cvttps_epi32(b_phase);
                let _out_v = [0.0f32; 8];
                let mut idx_arr = [0i32; 8];
                _mm256_storeu_si256(idx_arr.as_mut_ptr() as *mut __m256i, idx);

                for ch in 0..8 {
                    *outputs[ch].add(i) = table[idx_arr[ch] as usize % 1024];
                }

                b_phase = _mm256_add_ps(b_phase, b_inc);
                let mask = _mm256_cmp_ps(b_phase, lut_size, _CMP_GE_OQ);
                b_phase = _mm256_sub_ps(b_phase, _mm256_and_ps(mask, lut_size));
            }
            _mm256_storeu_ps(phase.as_mut_ptr(), b_phase);
        }
    }

    #[cfg(target_arch = "aarch64")]
    pub unsafe fn process_block_simd(&mut self, input: &[f32], output: &mut [f32]) {
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
        self.z1 = z1;
        self.z2 = z2;
    }
}

impl SimdBiquad {
    #[cfg(target_arch = "aarch64")]
    pub unsafe fn process_8_channels(&mut self, inputs: [*const f32; 8], outputs: [*mut f32; 8], len: usize) {
        use std::arch::aarch64::*;

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
            let x0 = vsetq_lane_f32(*inputs[0].add(i), vdupq_n_f32(0.0), 0);
            let x0 = vsetq_lane_f32(*inputs[1].add(i), x0, 1);
            let x0 = vsetq_lane_f32(*inputs[2].add(i), x0, 2);
            let x0 = vsetq_lane_f32(*inputs[3].add(i), x0, 3);

            let x1 = vsetq_lane_f32(*inputs[4].add(i), vdupq_n_f32(0.0), 0);
            let x1 = vsetq_lane_f32(*inputs[5].add(i), x1, 1);
            let x1 = vsetq_lane_f32(*inputs[6].add(i), x1, 2);
            let x1 = vsetq_lane_f32(*inputs[7].add(i), x1, 3);

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

            for ch in 0..4 { *outputs[ch].add(i) = out0[ch]; }
            for ch in 0..4 { *outputs[ch+4].add(i) = out1[ch]; }
        }

        vst1q_f32(self.z1.as_mut_ptr(), z1_0);
        vst1q_f32(self.z1.as_mut_ptr().add(4), z1_1);
        vst1q_f32(self.z2.as_mut_ptr(), z2_0);
        vst1q_f32(self.z2.as_mut_ptr().add(4), z2_1);
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx512f")]
    pub unsafe fn process_16_channels(&mut self, inputs: [*const f32; 16], outputs: [*mut f32; 16], len: usize) {
        use std::arch::x86_64::*;

        let b0 = _mm512_set1_ps(self.coeffs.b0);
        let b1 = _mm512_set1_ps(self.coeffs.b1);
        let b2 = _mm512_set1_ps(self.coeffs.b2);
        let a1 = _mm512_set1_ps(self.coeffs.a1);
        let a2 = _mm512_set1_ps(self.coeffs.a2);

        let mut z1 = _mm512_loadu_ps(self.z1.as_ptr());
        let mut z2 = _mm512_loadu_ps(self.z2.as_ptr());

        for i in 0..len {
            let x = _mm512_set_ps(
                *inputs[15].add(i), *inputs[14].add(i), *inputs[13].add(i), *inputs[12].add(i),
                *inputs[11].add(i), *inputs[10].add(i), *inputs[9].add(i), *inputs[8].add(i),
                *inputs[7].add(i), *inputs[6].add(i), *inputs[5].add(i), *inputs[4].add(i),
                *inputs[3].add(i), *inputs[2].add(i), *inputs[1].add(i), *inputs[0].add(i)
            );

            let y = _mm512_add_ps(_mm512_mul_ps(x, b0), z1);
            z1 = _mm512_add_ps(_mm512_sub_ps(_mm512_mul_ps(x, b1), _mm512_mul_ps(y, a1)), z2);
            z2 = _mm512_sub_ps(_mm512_mul_ps(x, b2), _mm512_mul_ps(y, a2));

            let mut out_v = [0.0f32; 16];
            _mm512_storeu_ps(out_v.as_mut_ptr(), y);
            for ch in 0..16 { *outputs[ch].add(i) = out_v[ch]; }
        }

        _mm512_storeu_ps(self.z1.as_mut_ptr(), z1);
        _mm512_storeu_ps(self.z2.as_mut_ptr(), z2);
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    pub unsafe fn process_8_channels(&mut self, inputs: [*const f32; 8], outputs: [*mut f32; 8], len: usize) {
        {
            use std::arch::x86_64::*;

            let b0 = _mm256_set1_ps(self.coeffs.b0);
            let b1 = _mm256_set1_ps(self.coeffs.b1);
            let b2 = _mm256_set1_ps(self.coeffs.b2);
            let a1 = _mm256_set1_ps(self.coeffs.a1);
            let a2 = _mm256_set1_ps(self.coeffs.a2);

            let mut z1 = _mm256_loadu_ps(self.z1.as_ptr());
            let mut z2 = _mm256_loadu_ps(self.z2.as_ptr());

            // Process in blocks of 8 samples when possible for even better utilization,
            // but since we are multi-channel (8 channels), we already process 8 samples (one from each) per iteration.
            // The previous gather/scatter was indeed inefficient.
            // If the inputs were interleaved, it would be much faster.
            // But they are separate buffers.

            for i in 0..len {
                let x = _mm256_set_ps(
                    *inputs[7].add(i), *inputs[6].add(i), *inputs[5].add(i), *inputs[4].add(i),
                    *inputs[3].add(i), *inputs[2].add(i), *inputs[1].add(i), *inputs[0].add(i)
                );

                let y = _mm256_add_ps(_mm256_mul_ps(x, b0), z1);
                z1 = _mm256_add_ps(_mm256_sub_ps(_mm256_mul_ps(x, b1), _mm256_mul_ps(y, a1)), z2);
                z2 = _mm256_sub_ps(_mm256_mul_ps(x, b2), _mm256_mul_ps(y, a2));

                let mut out_v = [0.0f32; 8];
                _mm256_storeu_ps(out_v.as_mut_ptr(), y);
                for ch in 0..8 { *outputs[ch].add(i) = out_v[ch]; }
            }

            _mm256_storeu_ps(self.z1.as_mut_ptr(), z1);
            _mm256_storeu_ps(self.z2.as_mut_ptr(), z2);
        }
    }
}
