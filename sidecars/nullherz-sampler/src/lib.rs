use audio_core::AudioProcessor;

pub struct SamplerSidecar {
    samples: Vec<Vec<f32>>,
    play_index: [Option<usize>; 16],
    #[allow(dead_code)]
    sample_rate: f32,
}

impl SamplerSidecar {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            samples: Vec::new(),
            play_index: [None; 16],
            sample_rate,
        }
    }

    pub fn load_sample(&mut self, data: Vec<f32>) -> usize {
        let id = self.samples.len();
        self.samples.push(data);
        id
    }

    pub fn trigger(&mut self, channel: usize, sample_id: usize) {
        if channel < 16 && sample_id < self.samples.len() {
            self.play_index[channel] = Some(0);
        }
    }
}

impl AudioProcessor for SamplerSidecar {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, _in: &[&[f32]], out: &mut [&mut [f32]], _context: &mut audio_core::processors::ProcessContext) {
        for ch in 0..out.len().min(16) {
            if let Some(mut idx) = self.play_index[ch] {
                if self.samples.is_empty() {
                    out[ch].fill(0.0);
                    continue;
                }
                let sample_data = &self.samples[0]; // For now play first loaded
                let len = out[ch].len();
                for val in out[ch].iter_mut().take(len) {
                    if idx < sample_data.len() {
                        *val = sample_data[idx];
                        idx += 1;
                    } else {
                        *val = 0.0;
                        self.play_index[ch] = None;
                        break;
                    }
                }
                if self.play_index[ch].is_some() { self.play_index[ch] = Some(idx); }
            } else {
                out[ch].fill(0.0);
            }
        }
    }

    fn apply_command(&mut self, cmd: &nullherz_traits::Command) {
        if let nullherz_traits::Command::Play = cmd {
            self.trigger(0, 0);
        }
    }
}
