use nullherz_traits::AudioProcessor;

pub struct WavetableProcessor {
    inner: audio_dsp::WavetableOscillator,
}

impl WavetableProcessor {
    pub fn new(sample_rate: f32) -> Self {
        Self { inner: audio_dsp::WavetableOscillator::new(sample_rate) }
    }
}

impl nullherz_traits::MidiHandler for WavetableProcessor {}
impl nullherz_traits::CommandHandler for WavetableProcessor {}
impl nullherz_traits::TopologyHandler for WavetableProcessor {}
impl nullherz_traits::TelemetryProvider for WavetableProcessor {}
impl AudioProcessor for WavetableProcessor {
    fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        self.inner.set_sample_rate(config.sample_rate);
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        let num_channels = outputs.len().min(crate::MAX_CHANNELS);
        let len = if num_channels > 0 { outputs[0].len() } else { 0 };
        if len == 0 { return; }

        let fm_storage = [0.0f32; 128];
        let pm_storage = [0.0f32; 128];

        // Optimization: Use SIMD multi-channel path if exactly 8 channels are available
        if num_channels == 8 {
            let mut fm_ptrs = [std::ptr::null(); 8];
            let mut pm_ptrs = [std::ptr::null(); 8];
            let mut out_ptrs = [std::ptr::null_mut(); 8];

            let fm_default = if !inputs.is_empty() { inputs[0] } else { &fm_storage[..len] };
            let pm_default = if inputs.len() > 1 { inputs[1] } else { &pm_storage[..len] };

            for (ch, (fm_ptr, (pm_ptr, out_ptr))) in fm_ptrs.iter_mut().zip(pm_ptrs.iter_mut().zip(out_ptrs.iter_mut())).enumerate() {
                *fm_ptr = fm_default.as_ptr();
                *pm_ptr = pm_default.as_ptr();
                *out_ptr = outputs[ch].as_mut_ptr();
            }

            self.inner.process_8_channels(fm_ptrs, pm_ptrs, out_ptrs, len);
            return;
        }

        for (ch, output) in outputs.iter_mut().enumerate().take(num_channels) {
            let fm = if !inputs.is_empty() { inputs[0] } else { &fm_storage[..len] };
            let pm = if inputs.len() > 1 { inputs[1] } else { &pm_storage[..len] };
            self.inner.process_scalar(ch, fm, pm, output);
        }
    }
}
