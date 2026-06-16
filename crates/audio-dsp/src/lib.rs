pub mod filters;
pub mod oscillators;
pub mod spectral;
pub mod util;
pub mod simd_vec;

pub use filters::*;
pub use oscillators::*;
pub use spectral::*;
pub use util::*;

pub trait DspKernel: Send + Clone {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]);
    fn reset(&mut self) {}
    fn set_parameter(&mut self, _id: u32, _value: f32, _ramp_samples: u32) {}
}

/// A SIMD Summing Node that mixes up to 16 input buffers into one output.
pub struct SummingNode {
    pub gain: f32,
}

impl Default for SummingNode {
    fn default() -> Self {
        Self::new()
    }
}

impl SummingNode {
    pub fn new() -> Self { Self { gain: 1.0 } }

    pub fn process_16_to_1(&self, inputs: &[&[f32]], output: &mut [f32]) {
        let len = output.len();
        output.fill(0.0);
        let g = self.gain;

        for input in inputs {
            let input_len = input.len();
            let process_len = len.min(input_len);
            for i in 0..process_len {
                output[i] += input[i] * g;
            }
        }
    }

    pub fn process_16_to_1_simd(&self, inputs: &[&[f32]], output: &mut [f32]) {
        use wide::*;
        use crate::simd_vec::*;
        let len = output.len();
        output.fill(0.0);
        let b_gain = f32x8::from(self.gain);

        for input in inputs {
            let input_len = input.len();
            let process_len = len.min(input_len);
            let mut i = 0;
            while i + 8 <= process_len {
                let v_in = load_f32x8(input, i);
                let v_out = load_f32x8(output, i);
                let res = v_out + (v_in * b_gain);
                store_f32x8(output, i, res);
                i += 8;
            }
            while i < process_len {
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

impl Default for Crossfader {
    fn default() -> Self {
        Self::new()
    }
}

impl Crossfader {
    pub fn new() -> Self { Self { position: 0.5 } }
    pub fn set_position(&mut self, pos: f32) { self.position = pos.clamp(0.0, 1.0); }

    pub fn process_block(&self, input_a: &[f32], input_b: &[f32], output: &mut [f32]) {
        let gain_b = self.position.sqrt();
        let gain_a = (1.0 - self.position).sqrt();

        for i in 0..output.len() {
            output[i] = input_a[i] * gain_a + input_b[i] * gain_b;
        }
    }

    pub fn process_block_simd(&self, input_a: &[f32], input_b: &[f32], output: &mut [f32]) {
        use wide::*;
        use crate::simd_vec::*;
        let len = output.len();
        let gain_b = self.position.sqrt();
        let gain_a = (1.0 - self.position).sqrt();
        let b_gain_b = f32x8::from(gain_b);
        let b_gain_a = f32x8::from(gain_a);

        let mut i = 0;
        while i + 8 <= len {
            let va = load_f32x8(input_a, i);
            let vb = load_f32x8(input_b, i);
            let res = (va * b_gain_a) + (vb * b_gain_b);
            store_f32x8(output, i, res);
            i += 8;
        }
        while i < len {
            output[i] = input_a[i] * gain_a + input_b[i] * gain_b;
            i += 1;
        }
    }
}

/// A high-performance Gain processor with parameter smoothing.
#[derive(Debug, Clone, Copy)]
pub struct Gain {
    pub current_gain: f32,
    pub target_gain: f32,
    pub _smoothing_factor: f32,
    pub ramp_remaining: u32,
    pub ramp_step: f32,
}

impl DspKernel for Gain {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        self.process_block(inputs[0], outputs[0]);
    }

    fn set_parameter(&mut self, id: u32, value: f32, ramp_samples: u32) {
        if id == 0 {
            self.set_gain(value, ramp_samples);
        }
    }
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
    pub(crate) twiddles: Vec<(f32, f32)>,
}

impl SimdFft {
    pub fn new(size: usize) -> Self {
        assert!(size.is_power_of_two(), "FFT size must be a power of two");
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

#[cfg(test)]
mod dsp_tests {
    use super::*;

    #[test]
    fn test_spectral_processor_convolution() {
        let mut proc = SpectralProcessor::new(128);
        let mut ir = vec![0.0; 128];
        ir[0] = 1.0; // Impulse
        proc.set_ir(&ir);

        let input = vec![0.5; 256];
        let mut output = vec![0.0; 256];
        proc.process_overlap_add(&input, &mut output);

        // Convolution with impulse should return the same signal (with some OLA delay/windowing artifacts)
        // For simplicity, we just check that it's not all zeros and roughly the same magnitude
        let sum: f32 = output.iter().sum();
        assert!(sum > 10.0);
    }
}

#[cfg(test)]
mod wavetable_tests {
    use super::*;

    #[test]
    fn test_lagrange_resampler() {
        let mut resampler = util::LagrangeResampler::new();
        let val = resampler.process_sample(1.0, 0.5);
        assert!(val != 0.0);
    }

    #[test]
    fn test_wavetable_integrity() {
        let mut osc = oscillators::WavetableOscillator::new(44100.0);
        osc.set_frequency(0, 440.0);
        let mut out = vec![0.0; 1024];
        let fm = vec![0.0; 1024];
        let pm = vec![0.0; 1024];
        osc.process_scalar(0, &fm, &pm, &mut out);

        let sum: f32 = out.iter().map(|s| s.abs()).sum();
        assert!(sum > 10.0);

        for &s in &out {
            assert!((-1.05..=1.05).contains(&s));
        }
    }

    #[test]
    fn test_gain_ramping() {
        let mut gain = Gain::new(0.0, 0.1);
        gain.set_gain(1.0, 100);
        let input = vec![1.0; 100];
        let mut output = vec![0.0; 100];
        gain.process_block(&input, &mut output);
        assert!((gain.current_gain - 1.0).abs() < 1e-6);
        assert!(output[0] > 0.0 && output[0] < 1.0);
        assert!((output[99] - 1.0).abs() < 0.02); // last sample should be near 1.0
    }

    #[test]
    fn test_summing_node_simd() {
        let node = SummingNode::new();
        let input1 = vec![0.1; 64];
        let input2 = vec![0.2; 64];
        let mut output_scalar = vec![0.0; 64];
        let mut output_simd = vec![0.0; 64];

        node.process_16_to_1(&[&input1, &input2], &mut output_scalar);
        node.process_16_to_1_simd(&[&input1, &input2], &mut output_simd);

        for i in 0..64 {
            assert!((output_scalar[i] - output_simd[i]).abs() < 1e-6);
            assert!((output_scalar[i] - 0.3).abs() < 1e-6);
        }
    }

    #[test]
    fn test_crossfader_simd() {
        let mut xfade = Crossfader::new();
        xfade.set_position(0.5);
        let input_a = vec![1.0; 64];
        let input_b = vec![2.0; 64];
        let mut output_scalar = vec![0.0; 64];
        let mut output_simd = vec![0.0; 64];

        xfade.process_block(&input_a, &input_b, &mut output_scalar);
        xfade.process_block_simd(&input_a, &input_b, &mut output_simd);

        for i in 0..64 {
            assert!((output_scalar[i] - output_simd[i]).abs() < 1e-6);
        }
    }
}
