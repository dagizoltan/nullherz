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
            window[i] = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / self.fft_size as f32).cos());
        }

        let mut mags = vec![0.0f32; num_bins];
        let mut phases = vec![0.0f32; num_bins];
        let mut flux_series = Vec::new();

        for i in (0..buffer.len().saturating_sub(self.fft_size)).step_by(self.hop_size) {
            re.fill(0.0);
            im.fill(0.0);

            let chunk = &buffer[i..i + self.fft_size];
            for (w_idx, &s) in chunk.iter().enumerate() {
                re[w_idx] = s * window[w_idx];
            }

            self.fft.process(&mut re, &mut im);

            mags.fill(0.0);
            phases.fill(0.0);
            let mut spectral_flux = 0.0f32;
            let mut low_freq_flux_raw = 0.0f32;
            let mut high_freq_flux_raw = 0.0f32;
            let mut complex_diff = 0.0f32;
            let mut phase_deviation = 0.0f32;

            let mut sub_low = 0.0f32;
            let mut low_mid = 0.0f32;
            let mut mid_high = 0.0f32;
            let mut high = 0.0f32;

            let bin_hz = self.sample_rate / self.fft_size as f32;

            for k in 0..num_bins {
                let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
                let phase = im[k].atan2(re[k]);
                mags[k] = mag;
                phases[k] = phase;

                // 1. Spectral Flux (rectified positive energy increase)
                let diff = mag - self.prev_mags[k];
                let freq = k as f32 * bin_hz;

                if diff > 0.0 {
                    spectral_flux += diff;
                    if freq < 300.0 {
                        low_freq_flux_raw += diff;
                    } else if freq > 3000.0 {
                        high_freq_flux_raw += diff;
                    }
                }

                // 2. Complex Spectral Difference
                let expected_phase = 2.0 * self.prev_phases[k] - self.prev_prev_phases[k];
                let target_re = self.prev_mags[k] * expected_phase.cos();
                let target_im = self.prev_mags[k] * expected_phase.sin();
                let c_diff_re = re[k] - target_re;
                let c_diff_im = im[k] - target_im;
                complex_diff += (c_diff_re * c_diff_re + c_diff_im * c_diff_im).sqrt();

                // 3. Phase deviation
                let phase_diff = (phase - expected_phase).abs();
                phase_deviation += phase_diff;

                // Band energy breakdown
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
            let rms = (chunk.iter().map(|&s| s * s).sum::<f32>() / self.fft_size as f32).sqrt();
            let rms_diff = (rms - self.prev_rms).max(0.0);

            // Total composite strength
            let combined_strength = spectral_flux * 0.4 + complex_diff * 0.3 + rms_diff * 5.0;

            self.prev_prev_phases.copy_from_slice(&self.prev_phases);
            self.prev_phases.copy_from_slice(&phases);
            self.prev_mags.copy_from_slice(&mags);
            self.prev_rms = rms;

            let norm_factor = spectral_flux.max(1e-6);
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
                low_freq_flux: (low_freq_flux_raw / norm_factor).clamp(0.0, 1.0),
                high_freq_flux: (high_freq_flux_raw / norm_factor).clamp(0.0, 1.0),
                complex_diff,
                phase_deviation: phase_deviation / num_bins as f32,
                confidence: (combined_strength * 2.0).clamp(0.0, 1.0),
                is_ghost: false,
                is_subdivision: false,
            };

            flux_series.push(candidate);
        }

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
}
