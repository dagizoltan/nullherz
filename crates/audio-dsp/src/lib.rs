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
