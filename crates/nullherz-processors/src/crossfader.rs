use nullherz_traits::AudioProcessor;

pub struct CrossfaderProcessor {
    inner: audio_dsp::Crossfader,
}

impl Default for CrossfaderProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl CrossfaderProcessor {
    pub fn new() -> Self { Self { inner: audio_dsp::Crossfader::new() } }
}

impl nullherz_traits::MidiHandler for CrossfaderProcessor {}
impl nullherz_traits::CommandHandler for CrossfaderProcessor {}
impl nullherz_traits::TopologyHandler for CrossfaderProcessor {}
impl nullherz_traits::TelemetryProvider for CrossfaderProcessor {}
impl AudioProcessor for CrossfaderProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if inputs.len() < 2 || outputs.is_empty() { return; }
        self.inner.process_block_simd(inputs[0], inputs[1], outputs[0]);
    }

    fn reset(&mut self) {}
}
