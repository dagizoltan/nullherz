use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorMetadata, ParameterMetadata, AudioConfig};
use audio_dsp::spectral::SpectralPipeline;
use wide::*;

pub struct KeySyncProcessor {
    pub id: u64,
    pipeline: SpectralPipeline,
    semitones: f32,
    ratio: f32,
    scratch_re: Vec<f32>,
    scratch_im: Vec<f32>,
}

impl KeySyncProcessor {
    pub fn new(id: u64, fft_size: usize) -> Self {
        Self {
            id,
            pipeline: SpectralPipeline::new(fft_size),
            semitones: 0.0,
            ratio: 1.0,
            scratch_re: vec![0.0; fft_size],
            scratch_im: vec![0.0; fft_size],
        }
    }

    pub fn set_semitones(&mut self, semitones: f32) {
        self.semitones = semitones;
        self.ratio = 2.0f32.powf(semitones / 12.0);
    }
}

impl nullherz_traits::SignalProcessor for KeySyncProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        let input = inputs[0];
        let output = &mut *outputs[0];

        let ratio = self.ratio;

        // Simple spectral pitch shifting
        let scratch_re = &mut self.scratch_re;
        let scratch_im = &mut self.scratch_im;

        self.pipeline.process(input, output, |re, im, n, _window, _fft| {
            if (ratio - 1.0).abs() < 0.001 { return; }

            scratch_re.fill(0.0);
            scratch_im.fill(0.0);

            // SIMD-optimized bin mapping with linear interpolation for higher quality
            let n_half = n / 2;
            let inv_ratio = 1.0 / ratio;

            // SIMD path for interpolation
            let mut i = 1;
            let v_inv_ratio = f32x8::from(inv_ratio);
            let v_indices = f32x8::from([0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);

            while i + 8 < n_half {
                let v_i = f32x8::from(i as f32) + v_indices;
                let v_source_pos = v_i * v_inv_ratio;
                let v_source_idx = v_source_pos.floor();
                let v_fraction = v_source_pos - v_source_idx;

                let source_indices: [f32; 8] = v_source_idx.into();
                let fractions: [f32; 8] = v_fraction.into();

                for (j, &s_idx_f) in source_indices.iter().enumerate() {
                    let s_idx = s_idx_f as usize;
                    if s_idx + 1 < n_half {
                        let f = fractions[j];
                        let r_val = re[s_idx] + (re[s_idx + 1] - re[s_idx]) * f;
                        let i_val = im[s_idx] + (im[s_idx + 1] - im[s_idx]) * f;

                        let idx = i + j;
                        scratch_re[idx] = r_val;
                        scratch_im[idx] = i_val;
                        scratch_re[n - idx] = r_val;
                        scratch_im[n - idx] = -i_val;
                    }
                }
                i += 8;
            }

            for i in i..n_half {
                let source_pos = i as f32 * inv_ratio;
                let source_idx = source_pos as usize;
                let fraction = source_pos - source_idx as f32;

                if source_idx + 1 < n_half {
                    // Linear interpolation between bins
                    let re0 = re[source_idx];
                    let re1 = re[source_idx + 1];
                    let im0 = im[source_idx];
                    let im1 = im[source_idx + 1];

                    let r_val = re0 + (re1 - re0) * fraction;
                    let i_val = im0 + (im1 - im0) * fraction;

                    scratch_re[i] = r_val;
                    scratch_im[i] = i_val;

                    // Conjugate symmetry
                    scratch_re[n - i] = r_val;
                    scratch_im[n - i] = -i_val;
                }
            }

            re.copy_from_slice(scratch_re);
            im.copy_from_slice(scratch_im);
        });
    }

    fn setup(&mut self, _config: AudioConfig) {
        self.pipeline.reset();
    }

    fn reset(&mut self) {
        self.pipeline.reset();
    }
}

impl nullherz_traits::MidiResponder for KeySyncProcessor {}
impl nullherz_traits::SnapshotProvider for KeySyncProcessor {}

impl AudioProcessor for KeySyncProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        if param_id == 0 {
            self.set_semitones(value);
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        if param_id == 0 { self.semitones } else { 0.0 }
    }

    fn metadata(&self) -> Option<ProcessorMetadata> {
        Some(ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 1,
            parameters: [
                ParameterMetadata {
                    id: 0,
                    name: *b"Semitones\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
                    min: -12.0,
                    max: 12.0,
                    default: 0.0,
                },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
                ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 },
            ],
        })
    }
}
