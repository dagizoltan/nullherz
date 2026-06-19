use nullherz_traits::{AudioProcessor, ProcessContext};
use audio_dsp::{SpectralPipeline, AlignedBuffer};

pub struct SpectralMorphProcessor {
    pipeline: SpectralPipeline,
    modulator_pipeline: SpectralPipeline,
    modulator_re: AlignedBuffer,
    modulator_im: AlignedBuffer,
    has_modulator_spectrum: bool,
}

impl SpectralMorphProcessor {
    pub fn new(fft_size: usize) -> Self {
        Self {
            pipeline: SpectralPipeline::new(fft_size),
            modulator_pipeline: SpectralPipeline::new(fft_size),
            modulator_re: AlignedBuffer::new(fft_size),
            modulator_im: AlignedBuffer::new(fft_size),
            has_modulator_spectrum: false,
        }
    }
}

impl AudioProcessor for SpectralMorphProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn reset(&mut self) {
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.len() < 2 || outputs.is_empty() { return; }

        let carrier = inputs[0];
        let modulator = inputs[1];
        let output = &mut outputs[0];

        let modulator_re_ptr = self.modulator_re.as_mut_ptr();
        let modulator_im_ptr = self.modulator_im.as_mut_ptr();
        let has_mod_ptr = &mut self.has_modulator_spectrum as *mut bool;
        let n = self.pipeline.fft.size;

        // Modulator Analysis
        let mut dummy_out = [0.0; 256];
        let dummy_out_slice = &mut dummy_out[..modulator.len().min(256)];

        self.modulator_pipeline.process(modulator, dummy_out_slice, |re, im, _n, _window, _fft| {
            unsafe {
                std::ptr::copy_nonoverlapping(re.as_ptr(), modulator_re_ptr, n);
                std::ptr::copy_nonoverlapping(im.as_ptr(), modulator_im_ptr, n);
                *has_mod_ptr = true;
            }
        });

        // Carrier Processing
        let modulator_re_ref = &self.modulator_re;
        let modulator_im_ref = &self.modulator_im;
        let has_mod_ref = &self.has_modulator_spectrum;

        self.pipeline.process(carrier, output, |re, im, n, _window, _fft| {
            if *has_mod_ref {
                // Magnitude cross-multiply (classic vocoder)
                for i in 0..n {
                    let m_mag = (modulator_re_ref[i] * modulator_re_ref[i] + modulator_im_ref[i] * modulator_im_ref[i]).sqrt();
                    re[i] *= m_mag;
                    im[i] *= m_mag;
                }
            }
        });
    }
}
