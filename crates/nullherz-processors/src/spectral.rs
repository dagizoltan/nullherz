use nullherz_traits::{AudioProcessor, ProcessorMetadata, ParameterMetadata};

pub struct SpectralProcessor {
    pub id: u64,
    inner: audio_dsp::SpectralProcessor,
}

impl SpectralProcessor {
    pub fn new(id: u64, fft_size: usize) -> Self {
        Self {
            id,
            inner: audio_dsp::SpectralProcessor::new(fft_size),
        }
    }
}

impl nullherz_traits::SignalProcessor for SpectralProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        // Underlying audio-dsp::SpectralPipeline handles arbitrary input lengths
        // by buffering internally. We directly pass the slices.
        // It also handles internal cross-fading and overlap-add logic for block boundaries.
        self.inner.process_overlap_add(inputs[0], outputs[0]);
    }

    fn latency_samples(&self) -> usize {
        self.inner.pipeline.fft.size
    }

    fn reset(&mut self) {
        self.inner.pipeline.reset();
    }
}

impl nullherz_traits::MidiResponder for SpectralProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for SpectralProcessor { }

impl AudioProcessor for SpectralProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn metadata(&self) -> Option<ProcessorMetadata> {
        Some(ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 0,
            parameters: [ParameterMetadata {
                id: 0,
                name: [0; 32],
                min: 0.0,
                max: 0.0,
                default: 0.0,
            }; 16],
        })
    }

    fn apply_topology_mutation(&mut self, mutation: nullherz_traits::TopologyMutation) {
        if let nullherz_traits::TopologyMutation::AddSource { buffer, .. } = mutation {
            // Use AddSource to set the Impulse Response for spectral convolution
            self.inner.set_ir(&buffer);
        }
    }
}
