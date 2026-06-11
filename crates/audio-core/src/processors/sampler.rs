use crate::processors::AudioProcessor;
use audio_dsp::SamplerVoice;

#[derive(Debug)]
pub struct SamplerProcessor {
    voices: Vec<SamplerVoice>,
    sample_buffer: std::sync::Arc<Vec<f32>>,
}

impl SamplerProcessor {
    pub fn new(_id: u64) -> Self {
        let voices = (0..8).map(|_| SamplerVoice::new()).collect();
        Self {
            voices,
            sample_buffer: std::sync::Arc::new(Vec::new()),
        }
    }

    pub fn set_sample(&mut self, buffer: Vec<f32>) {
        self.sample_buffer = std::sync::Arc::new(buffer);
    }
}

impl AudioProcessor for SamplerProcessor {
    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut crate::processors::ProcessContext) {
        if outputs.is_empty() { return; }

        for output in outputs.iter_mut() {
            output.fill(0.0);
            for voice in self.voices.iter_mut() {
                voice.process_block(output);
            }
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
