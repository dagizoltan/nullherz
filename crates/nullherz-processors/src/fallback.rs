use nullherz_traits::{AudioProcessor, ProcessContext};

/// A zero-overhead identity processor used as a soft fallback for failing DSP nodes.
pub struct FallbackProcessor {
    id: u64,
}

impl FallbackProcessor {
    pub fn new(id: u64) -> Self {
        Self { id }
    }
}

impl nullherz_traits::SignalProcessor for FallbackProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        let num_channels = inputs.len().min(outputs.len());
        for ch in 0..num_channels {
            outputs[ch].copy_from_slice(inputs[ch]);
        }
        // Fill remaining output channels with silence
        for ch in num_channels..outputs.len() {
            outputs[ch].fill(0.0);
        }
    }

    fn reset(&mut self) {}
}

impl nullherz_traits::MidiResponder for FallbackProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }
impl nullherz_traits::SnapshotProvider for FallbackProcessor {}

impl AudioProcessor for FallbackProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 0,
            parameters: [nullherz_traits::ParameterMetadata {
                id: 0,
                name: [0; 32],
                min: 0.0,
                max: 0.0,
                default: 0.0,
            }; 16],
        })
    }
}
