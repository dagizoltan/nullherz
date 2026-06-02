use crate::processors::AudioProcessor;

pub struct WavetableProcessor {
    inner: audio_dsp::WavetableOscillator,
}

impl WavetableProcessor {
    pub fn new(sample_rate: f32) -> Self {
        Self { inner: audio_dsp::WavetableOscillator::new(sample_rate) }
    }
}

impl AudioProcessor for WavetableProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        let num_channels = outputs.len().min(crate::MAX_CHANNELS);
        let len = if num_channels > 0 { outputs[0].len() } else { 0 };
        if len == 0 { return; }

        let fm_storage = [0.0f32; 128];
        let pm_storage = [0.0f32; 128];

        for ch in 0..num_channels {
            let fm = if inputs.len() > 0 { inputs[0] } else { &fm_storage[..len] };
            let pm = if inputs.len() > 1 { inputs[1] } else { &pm_storage[..len] };
            self.inner.process_scalar(ch, fm, pm, outputs[ch]);
        }
    }
}

pub struct SpectralProcessor {
    inner: audio_dsp::SpectralProcessor,
}

impl SpectralProcessor {
    pub fn new(fft_size: usize) -> Self {
        Self { inner: audio_dsp::SpectralProcessor::new(fft_size) }
    }
}

impl AudioProcessor for SpectralProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        self.inner.process_overlap_add(inputs[0], outputs[0]);
    }
}

pub struct ModulationProcessor {
    pub target_id: u64,
    pub param_id: u32,
    pub scale: f32,
    pub offset: f32,
}

impl ModulationProcessor {
    pub fn new(target_id: u64, param_id: u32, scale: f32, offset: f32) -> Self {
        Self { target_id, param_id, scale, offset }
    }
}

impl AudioProcessor for ModulationProcessor {
    fn process(&mut self, inputs: &[&[f32]], _outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() { return; }
        let cv = inputs[0];
        if cv.is_empty() { return; }

        let _avg_cv: f32 = cv.iter().sum::<f32>() / cv.len() as f32;
        let _val = _avg_cv * self.scale + self.offset;
    }
}
