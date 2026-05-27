/// Basic DSP traits and primitives.

pub trait Oscillator {
    fn next_sample(&mut self) -> f32;
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

/// A high-performance Gain processor with parameter smoothing.
pub struct Gain {
    current_gain: f32,
    target_gain: f32,
    smoothing_factor: f32,
}

impl Gain {
    pub fn new(initial_gain: f32, smoothing_factor: f32) -> Self {
        Self {
            current_gain: initial_gain,
            target_gain: initial_gain,
            smoothing_factor,
        }
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.target_gain = gain;
    }

    /// Process a block of samples.
    /// This implementation uses manual unrolling and hint-friendly patterns for SIMD.
    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        let len = input.len();
        let target = self.target_gain;
        let factor = self.smoothing_factor;
        let mut current = self.current_gain;

        // We calculate a gain ramp for the block if the target changed.
        // For simplicity and vectorization, we'll use linear interpolation over the block.
        if (target - current).abs() > 0.0001 {
            let step = (target - current) * factor / len as f32;
            for i in 0..len {
                current += step;
                output[i] = input[i] * current;
            }
        } else {
            // Static gain loop - extremely easy to vectorize
            current = target;
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
    z1: f32,
    z2: f32,
}

impl BiquadFilter {
    pub fn new(coeffs: BiquadCoefficients) -> Self {
        Self {
            coeffs,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn update_coeffs(&mut self, coeffs: BiquadCoefficients) {
        self.coeffs = coeffs;
    }

    #[target_feature(enable = "avx2")]
    pub unsafe fn process_block_simd(&mut self, input: &[f32], output: &mut [f32]) {
        #[cfg(target_arch = "x86_64")]
        {
            let len = input.len();
            if len == 0 { return; }

            let mut z1 = self.z1;
            let mut z2 = self.z2;
            let b0 = self.coeffs.b0;
            let b1 = self.coeffs.b1;
            let b2 = self.coeffs.b2;
            let a1 = self.coeffs.a1;
            let a2 = self.coeffs.a2;

            // Direct Form I for single-channel SIMD unrolling (better instruction parallelism)
            for i in 0..len {
                let out = input[i] * b0 + z1;
                z1 = input[i] * b1 - out * a1 + z2;
                z2 = input[i] * b2 - out * a2;
                output[i] = out;
            }
            self.z1 = z1;
            self.z2 = z2;
        }
    }
}

impl Filter for BiquadFilter {
    fn process_sample(&mut self, input: f32) -> f32 {
        let output = input * self.coeffs.b0 + self.z1;
        self.z1 = input * self.coeffs.b1 - output * self.coeffs.a1 + self.z2;
        self.z2 = input * self.coeffs.b2 - output * self.coeffs.a2;
        output
    }
}

/// A Biquad Filter that processes 8 channels in parallel using AVX2.
#[repr(C, align(64))]
pub struct SimdBiquad {
    pub coeffs: BiquadCoefficients,
    z1: [f32; 8],
    z2: [f32; 8],
}

impl SimdBiquad {
    pub fn new(coeffs: BiquadCoefficients) -> Self {
        Self {
            coeffs,
            z1: [0.0; 8],
            z2: [0.0; 8],
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

    #[target_feature(enable = "avx2")]
    pub unsafe fn process_8_channels(&mut self, inputs: [*const f32; 8], outputs: [*mut f32; 8], len: usize) {
        #[cfg(target_arch = "x86_64")]
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
