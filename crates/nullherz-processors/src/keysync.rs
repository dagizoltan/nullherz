use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorMetadata, ParameterMetadata, AudioConfig};
use audio_dsp::spectral::SpectralPipeline;

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

            for i in 0..n/2 {
                let target = (i as f32 * ratio) as usize;
                if target < n/2 {
                    scratch_re[target] = re[i];
                    scratch_im[target] = im[i];

                    // Conjugate for IFFT symmetry
                    if target > 0 {
                        scratch_re[n - target] = re[i];
                        scratch_im[n - target] = -im[i];
                    }
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
