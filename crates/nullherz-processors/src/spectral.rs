use nullherz_traits::AudioProcessor;

pub struct SpectralProcessor {
    inner: audio_dsp::SpectralProcessor,
}

impl SpectralProcessor {
    pub fn new(fft_size: usize) -> Self {
        Self { inner: audio_dsp::SpectralProcessor::new(fft_size) }
    }
}

impl nullherz_traits::SignalProcessor for SpectralProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }

        let in_len = inputs[0].len();
        let out_len = outputs[0].len();

        // Hardened: handle mismatched buffer boundaries via zero-padding surrogate
        if in_len == out_len {
            self.inner.process_overlap_add(inputs[0], outputs[0]);
        } else {
            let mut padded_in = [0.0f32; 256];
            let len = in_len.min(256);
            padded_in[..len].copy_from_slice(&inputs[0][..len]);

            let mut temp_out = [0.0f32; 256];
            self.inner.process_overlap_add(&padded_in, &mut temp_out);

            let out_copy_len = out_len.min(256);
            outputs[0][..out_copy_len].copy_from_slice(&temp_out[..out_copy_len]);
        }
    }
}

impl nullherz_traits::MidiResponder for SpectralProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for SpectralProcessor { }

impl AudioProcessor for SpectralProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
