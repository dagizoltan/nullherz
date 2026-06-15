use nullherz_traits::AudioProcessor;

pub struct SummingProcessor {
    inner: audio_dsp::SummingNode,
}

impl Default for SummingProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SummingProcessor {
    pub fn new() -> Self { Self { inner: audio_dsp::SummingNode::new() } }
}

impl nullherz_traits::MidiHandler for SummingProcessor {}
impl nullherz_traits::CommandHandler for SummingProcessor {}
impl nullherz_traits::TopologyHandler for SummingProcessor {}
impl nullherz_traits::TelemetryProvider for SummingProcessor {}
impl AudioProcessor for SummingProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if outputs.is_empty() { return; }
        self.inner.process_16_to_1_simd(inputs, outputs[0]);
    }
}
