use nullherz_traits::{AudioProcessor, MidiHandler, CommandHandler, TopologyHandler, TelemetryProvider};

pub struct SpectralProcessor {
    inner: audio_dsp::SpectralProcessor,
}

impl SpectralProcessor {
    pub fn new(fft_size: usize) -> Self {
        Self { inner: audio_dsp::SpectralProcessor::new(fft_size) }
    }
}

impl MidiHandler for SpectralProcessor {}
impl CommandHandler for SpectralProcessor {
    fn apply_command(&mut self, _command: &nullherz_traits::ProcessorCommand) {}
    fn set_parameter(&mut self, _param_id: u32, _value: f32, _ramp_duration_samples: u32) {}
}
impl TopologyHandler for SpectralProcessor {}
impl TelemetryProvider for SpectralProcessor {}
impl AudioProcessor for SpectralProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        // For prototype, we ensure lengths match.
        let len = inputs[0].len().min(outputs[0].len());
        self.inner.process_overlap_add(&inputs[0][..len], &mut outputs[0][..len]);
    }
}
