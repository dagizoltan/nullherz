use nullherz_traits::AudioProcessor;
use audio_dsp::SamplerVoice;

#[derive(Debug)]
pub struct SamplerProcessor {
    voices: Vec<SamplerVoice>,
    sample_buffer: std::sync::Arc<Vec<f32>>,
    render_buffer: [f32; ipc_layer::MAX_BLOCK_SIZE],
}

impl SamplerProcessor {
    pub fn new(_id: u64) -> Self {
        let voices = (0..8).map(|_| SamplerVoice::new()).collect();
        Self {
            voices,
            sample_buffer: std::sync::Arc::new(Vec::new()),
            render_buffer: [0.0; ipc_layer::MAX_BLOCK_SIZE],
        }
    }

    pub fn set_sample(&mut self, buffer: Vec<f32>) {
        self.sample_buffer = std::sync::Arc::new(buffer);
    }
}

impl AudioProcessor for SamplerProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if outputs.is_empty() { return; }
        let num_samples = outputs[0].len();
        if num_samples == 0 { return; }

        // Production Hardening: Ensure we don't overflow the fixed-size render_buffer.
        // The engine should enforce this, but we protect the processor here.
        let num_samples = num_samples.min(ipc_layer::MAX_BLOCK_SIZE);

        // Render all voices into the temporary render_buffer once per cycle.
        self.render_buffer[..num_samples].fill(0.0);
        let render_slice = &mut self.render_buffer[..num_samples];
        for voice in self.voices.iter_mut() {
            voice.process_block(render_slice);
        }

        // Copy the rendered result to all output channels.
        for output in outputs.iter_mut() {
            output.copy_from_slice(render_slice);
        }
    }

    fn apply_midi(&mut self, event: ipc_layer::MidiEvent) {
        let status = event.status & 0xF0;
        #[allow(clippy::collapsible_if)]
        if status == 0x90 && event.data2 > 0 {
            if let Some(voice) = self.voices.iter_mut().find(|v| !v.is_active) {
                let freq = 440.0 * 2.0f32.powf((event.data1 as f32 - 69.0) / 12.0);
                let playback_rate = freq / 440.0;
                let velocity = event.data2 as f32 / 127.0;
                voice.trigger(self.sample_buffer.clone(), playback_rate, velocity);
            }
        }
    }
}
