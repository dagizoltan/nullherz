/// Programmatic Neural Audio Dataset Generator
/// Generates synthetic excitation signals (Sine Sweeps, White/Pink Noise, Multi-Tones)
/// for offline neural DSP model training pairs (x[t], y[t]).
pub struct NeuralDatasetGenerator;

impl NeuralDatasetGenerator {
    /// Generates a logarithmic / exponential sine sweep from `f_start` to `f_end` Hz.
    pub fn generate_exponential_sweep(sample_rate: f32, duration_secs: f32, f_start: f32, f_end: f32, amplitude: f32) -> Vec<f32> {
        let total_samples = (sample_rate * duration_secs) as usize;
        let mut samples = vec![0.0f32; total_samples];

        let f_s = f_start.max(20.0);
        let f_e = f_end.min(sample_rate * 0.45);
        let log_ratio = (f_e / f_s).ln();

        for i in 0..total_samples {
            let t = i as f32 / sample_rate;
            let phase = f_s * duration_secs * ((t / duration_secs * log_ratio).exp() - 1.0) / log_ratio;
            samples[i] = (phase * std::f32::consts::TAU).sin() * amplitude;
        }

        samples
    }

    /// Generates Gaussian white noise bursts with envelope attacks and decays.
    pub fn generate_noise_bursts(sample_rate: f32, duration_secs: f32, burst_period_secs: f32, amplitude: f32) -> Vec<f32> {
        let total_samples = (sample_rate * duration_secs) as usize;
        let period_samples = (sample_rate * burst_period_secs) as usize;
        let mut samples = vec![0.0f32; total_samples];

        let mut seed: u32 = 12345;
        for i in 0..total_samples {
            // Fast LCG pseudo-random generator
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((seed as f32) / 4294967295.0) * 2.0 - 1.0;

            let pos_in_burst = i % period_samples;
            let burst_progress = pos_in_burst as f32 / period_samples as f32;
            let env = if burst_progress < 0.1 {
                burst_progress / 0.1
            } else if burst_progress < 0.5 {
                1.0 - (burst_progress - 0.1) / 0.4
            } else {
                0.0
            };

            samples[i] = noise * env * amplitude;
        }

        samples
    }

    /// Generates a multi-tone complex for intermodulation distortion (IMD) testing.
    pub fn generate_multitone_complex(sample_rate: f32, duration_secs: f32, frequencies: &[f32], amplitude: f32) -> Vec<f32> {
        let total_samples = (sample_rate * duration_secs) as usize;
        let mut samples = vec![0.0f32; total_samples];
        let num_tones = frequencies.len().max(1) as f32;

        for i in 0..total_samples {
            let t = i as f32 / sample_rate;
            let mut sum = 0.0f32;
            for &f in frequencies {
                sum += (t * f * std::f32::consts::TAU).sin();
            }
            samples[i] = (sum / num_tones) * amplitude;
        }

        samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_sweep_generation() {
        let sweep = NeuralDatasetGenerator::generate_exponential_sweep(48000.0, 1.0, 20.0, 20000.0, 0.8);
        assert_eq!(sweep.len(), 48000);
        assert!(sweep.iter().all(|s| s.is_finite()));
        assert!(sweep.iter().any(|&s| s.abs() > 0.5));
    }

    #[test]
    fn test_noise_burst_generation() {
        let noise = NeuralDatasetGenerator::generate_noise_bursts(48000.0, 1.0, 0.25, 0.9);
        assert_eq!(noise.len(), 48000);
        assert!(noise.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn test_multitone_generation() {
        let tones = NeuralDatasetGenerator::generate_multitone_complex(48000.0, 0.5, &[100.0, 440.0, 1000.0, 5000.0], 0.7);
        assert_eq!(tones.len(), 24000);
        assert!(tones.iter().all(|s| s.is_finite()));
    }
}
