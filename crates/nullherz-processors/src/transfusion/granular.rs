use std::sync::Arc;
use nullherz_traits::{AudioProcessor, ProcessContext};
use audio_dsp::SamplerVoice;

const MAX_GRAINS: usize = 16;

pub struct GranularProcessor {
    voices: Vec<SamplerVoice>,
    source_pool: Vec<Arc<Vec<f32>>>,
    render_buffer: [f32; ipc_layer::MAX_BLOCK_SIZE],
    density: f32,
    _grain_duration_ms: f32,
    next_grain_samples: f32,
    sample_rate: f32,
    rng_state: u64,
}

impl GranularProcessor {
    pub fn new(sample_rate: f32) -> Self {
        let voices = (0..MAX_GRAINS).map(|_| SamplerVoice::new()).collect();
        Self {
            voices,
            source_pool: Vec::new(),
            render_buffer: [0.0; ipc_layer::MAX_BLOCK_SIZE],
            density: 10.0, // Grains per second
            _grain_duration_ms: 100.0,
            next_grain_samples: 0.0,
            sample_rate,
            rng_state: 12345,
        }
    }

    fn next_rand(&mut self) -> f32 {
        self.rng_state = self.rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.rng_state >> 32) as f32 / 4294967296.0
    }

    pub fn add_source(&mut self, source: Arc<Vec<f32>>) {
        self.source_pool.push(source);
    }
}

impl AudioProcessor for GranularProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn reset(&mut self) {
        for v in self.voices.iter_mut() {
            v.is_active = false;
        }
    }

    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if outputs.is_empty() { return; }
        let num_samples = outputs[0].len();
        let num_samples = num_samples.min(ipc_layer::MAX_BLOCK_SIZE);

        if self.source_pool.is_empty() {
            for output in outputs.iter_mut() {
                output.fill(0.0);
            }
            return;
        }

        let mut samples_processed = 0;
        while samples_processed < num_samples {
            if self.next_grain_samples <= 0.0 {
                let r1 = self.next_rand();
                let r2 = self.next_rand();
                let r3 = self.next_rand();

                // Trigger new grain
                if let Some(voice) = self.voices.iter_mut().find(|v| !v.is_active) {
                    let source_idx = (r1 * self.source_pool.len() as f32) as usize % self.source_pool.len();
                    let source = self.source_pool[source_idx].clone();

                    let source_len = source.len() as f32;
                    let start_pos = r2 * (source_len - self.sample_rate * 0.5).max(0.0);
                    let playback_rate = 0.5 + r3; // 0.5 to 1.5

                    voice.trigger(source, playback_rate, 1.0);
                    voice.play_head = start_pos;
                }

                let interval_sec = 1.0 / self.density.max(0.1);
                self.next_grain_samples = interval_sec * self.sample_rate;
            }

            let chunk = (num_samples - samples_processed).min(self.next_grain_samples.ceil() as usize);
            self.next_grain_samples -= chunk as f32;
            samples_processed += chunk;
        }

        self.render_buffer[..num_samples].fill(0.0);
        let render_slice = &mut self.render_buffer[..num_samples];

        for voice in self.voices.iter_mut() {
            voice.process_block(render_slice);
        }

        for output in outputs.iter_mut() {
            output.copy_from_slice(render_slice);
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        match param_id {
            0 => self.density = value.clamp(0.1, 100.0),
            1 => self._grain_duration_ms = value.clamp(1.0, 1000.0),
            _ => {}
        }
    }
}
