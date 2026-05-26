/// Basic DSP traits and primitives.

pub trait Oscillator {
    fn next_sample(&mut self) -> f32;
}

pub trait Filter {
    fn process_sample(&mut self, input: f32) -> f32;
}

pub struct SineOscillator {
    phase: f32,
    phase_inc: f32,
    sample_rate: f32,
}

impl SineOscillator {
    pub fn new(sample_rate: f32, frequency: f32) -> Self {
        Self {
            phase: 0.0,
            phase_inc: 2.0 * std::f32::consts::PI * frequency / sample_rate,
            sample_rate,
        }
    }

    pub fn set_frequency(&mut self, frequency: f32) {
        self.phase_inc = 2.0 * std::f32::consts::PI * frequency / self.sample_rate;
    }
}

impl Oscillator for SineOscillator {
    fn next_sample(&mut self) -> f32 {
        let sample = self.phase.sin();
        self.phase += self.phase_inc;
        if self.phase >= 2.0 * std::f32::consts::PI {
            self.phase -= 2.0 * std::f32::consts::PI;
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

    /// Process a block of samples with SIMD-friendly loop.
    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        let len = input.len();
        for i in 0..len {
            // Simple linear smoothing for each sample
            self.current_gain += (self.target_gain - self.current_gain) * self.smoothing_factor;
            output[i] = input[i] * self.current_gain;
        }
    }
}
