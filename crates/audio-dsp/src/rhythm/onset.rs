use alloc::vec::Vec;
use alloc::vec;
use num_traits::Float;
use super::types::{BandEnergy, OnsetCandidate};
use crate::SimdFft;

pub struct MultiFeatureOnsetDetector {
    fft_size: usize,
    hop_size: usize,
    sample_rate: f32,
    fft: SimdFft,
    prev_mags: Vec<f32>,
    prev_phases: Vec<f32>,
    prev_prev_phases: Vec<f32>,
    prev_rms: f32,
}

impl MultiFeatureOnsetDetector {
    pub fn new(sample_rate: f32) -> Self {
        let fft_size = 1024;
        let hop_size = 256;
        Self {
            fft_size,
            hop_size,
            sample_rate,
            fft: SimdFft::new(fft_size),
            prev_mags: vec![0.0; fft_size / 2],
            prev_phases: vec![0.0; fft_size / 2],
            prev_prev_phases: vec![0.0; fft_size / 2],
            prev_rms: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    pub fn process_buffer(&mut self, buffer: &[f32]) -> Vec<OnsetCandidate> {
        let mut candidates = Vec::new();
        if buffer.len() < self.fft_size {
            return candidates;
        }

        let num_bins = self.fft_size / 2;
        let mut re = vec![0.0f32; self.fft_size];
        let mut im = vec![0.0f32; self.fft_size];
        let mut window = vec![0.0f32; self.fft_size];
        for i in 0..self.fft_size {
            window[i] = 0.5 * (1.0 - Float::cos(2.0 * core::f32::consts::PI * i as f32 / self.fft_size as f32));
        }

        let mut flux_series = Vec::new();

        for i in (0..buffer.len().saturating_sub(self.fft_size)).step_by(self.hop_size) {
            re.fill(0.0);
            im.fill(0.0);

            let chunk = &buffer[i..i + self.fft_size];
            for (w_idx, &s) in chunk.iter().enumerate() {
                re[w_idx] = s * window[w_idx];
            }

            self.fft.process(&mut re, &mut im);

            let mut mags = vec![0.0f32; num_bins];
            let mut phases = vec![0.0f32; num_bins];
            let mut spectral_flux = 0.0f32;
            let mut complex_diff = 0.0f32;
            let mut phase_deviation = 0.0f32;

            let mut sub_low = 0.0f32;
            let mut low_mid = 0.0f32;
            let mut mid_high = 0.0f32;
            let mut high = 0.0f32;

            let bin_hz = self.sample_rate / self.fft_size as f32;

            for k in 0..num_bins {
                let mag = Float::sqrt(re[k] * re[k] + im[k] * im[k]);
                let phase = Float::atan2(im[k], re[k]);
                mags[k] = mag;
                phases[k] = phase;

                // 1. Spectral Flux (rectified positive energy increase)
                let diff = mag - self.prev_mags[k];
                if diff > 0.0 {
                    spectral_flux += diff;
                }

                // 2. Complex Spectral Difference
                let expected_phase = 2.0 * self.prev_phases[k] - self.prev_prev_phases[k];
                let target_re = self.prev_mags[k] * Float::cos(expected_phase);
                let target_im = self.prev_mags[k] * Float::sin(expected_phase);
                let c_diff_re = re[k] - target_re;
                let c_diff_im = im[k] - target_im;
                complex_diff += Float::sqrt(c_diff_re * c_diff_re + c_diff_im * c_diff_im);

                // 3. Phase deviation
                let phase_diff = Float::abs(phase - expected_phase);
                phase_deviation += phase_diff;

                // Band energy breakdown
                let freq = k as f32 * bin_hz;
                if freq < 100.0 {
                    sub_low += mag;
                } else if freq < 500.0 {
                    low_mid += mag;
                } else if freq < 4000.0 {
                    mid_high += mag;
                } else {
                    high += mag;
                }
            }

            // RMS Change
            let rms = Float::sqrt(chunk.iter().map(|&s| s * s).sum::<f32>() / self.fft_size as f32);
            let rms_diff = (rms - self.prev_rms).max(0.0);

            // Total composite strength
            let combined_strength = spectral_flux * 0.4 + complex_diff * 0.3 + rms_diff * 5.0;

            self.prev_prev_phases.copy_from_slice(&self.prev_phases);
            self.prev_phases.copy_from_slice(&phases);
            self.prev_mags.copy_from_slice(&mags);
            self.prev_rms = rms;

            let candidate = OnsetCandidate {
                frame: i as u64,
                time_sec: i as f64 / self.sample_rate as f64,
                strength: combined_strength,
                band_energy: BandEnergy {
                    sub_low,
                    low_mid,
                    mid_high,
                    high,
                },
                spectral_flux,
                complex_diff,
                phase_deviation: phase_deviation / num_bins as f32,
                confidence: (combined_strength * 2.0).clamp(0.0, 1.0),
                is_ghost: false,
                is_subdivision: false,
            };

            flux_series.push(candidate);
        }

        // Apply HPSS (Harmonic-Percussive Source Separation) & SuperFlux max-filtering over time-frequency frames
        Self::apply_hpss_and_superflux(&mut flux_series);

        // Peak picking over local adaptive threshold
        if flux_series.is_empty() {
            return candidates;
        }

        let win_size = 7;
        let mut strengths: Vec<f32> = flux_series.iter().map(|c| c.strength).collect();
        let max_str = strengths.iter().cloned().fold(0.0f32, f32::max).max(1e-6);
        for s in &mut strengths {
            *s /= max_str;
        }

        for idx in 0..flux_series.len() {
            let start = idx.saturating_sub(win_size / 2);
            let end = (idx + win_size / 2 + 1).min(flux_series.len());
            let local_mean = strengths[start..end].iter().sum::<f32>() / (end - start) as f32;
            let thresh = local_mean * 1.25 + 0.05;

            let val = strengths[idx];
            let is_local_max = (idx == 0 || val >= strengths[idx - 1])
                && (idx == flux_series.len() - 1 || val >= strengths[idx + 1]);

            if val > thresh && is_local_max {
                let mut cand = flux_series[idx].clone();
                cand.strength = val;
                cand.confidence = (val * 1.5).clamp(0.1, 1.0);
                candidates.push(cand);
            }
        }

        candidates
    }

    /// Apply Harmonic-Percussive Source Separation (HPSS) & SuperFlux max-filtering
    fn apply_hpss_and_superflux(series: &mut [OnsetCandidate]) {
        if series.len() < 3 { return; }

        let num_frames = series.len();
        let mut superflux_strengths = vec![0.0f32; num_frames];

        for i in 1..num_frames {
            let curr = &series[i];
            let prev = &series[i - 1];

            // 1. HPSS Percussive Emphasis: Weight percussive sub-low/low-mid transients over high-frequency vocal/harmonic vibrato
            let percussive_ratio = (curr.band_energy.sub_low * 2.5 + curr.band_energy.low_mid * 1.5)
                / (curr.band_energy.mid_high + curr.band_energy.high + 1e-4);
            let hpss_weight = (1.0 + Float::min(percussive_ratio, 3.0)) * 0.5;

            // 2. SuperFlux Max-Filtering: Compare current flux against local max neighborhood of previous frame to suppress vibrato
            let prev_max_flux = Float::max(prev.spectral_flux, prev.complex_diff * 0.5);
            let superflux_diff = Float::max(0.0, curr.spectral_flux - prev_max_flux * 0.85);

            superflux_strengths[i] = (superflux_diff * 0.6 + curr.strength * 0.4) * hpss_weight;
        }

        for (i, cand) in series.iter_mut().enumerate() {
            cand.strength = superflux_strengths[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onset_detector_hpss_and_superflux() {
        let mut detector = MultiFeatureOnsetDetector::new(48000.0);
        let mut buffer = vec![0.0f32; 48000];

        // Synthesize 4 impulses (kick drum transients) at 0.5s intervals
        for &pulse_sample in &[0, 24000, 36000] {
            if pulse_sample + 100 < buffer.len() {
                for i in 0..100 {
                    buffer[pulse_sample + i] = (i as f32 * 0.1).sin();
                }
            }
        }

        let candidates = detector.process_buffer(&buffer);
        assert!(!candidates.is_empty(), "Detector should identify onsets from impulses");
    }
}
