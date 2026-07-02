use audio_core::AudioProcessor;

pub struct SamplerSidecar {
    samples: Vec<Vec<f32>>,
    play_index: [Option<(usize, usize)>; 16], // (sample_id, index)
}

impl Default for SamplerSidecar {
    fn default() -> Self {
        Self::new()
    }
}

impl SamplerSidecar {
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
            play_index: [None; 16],
        }
    }

    pub fn load_sample(&mut self, data: Vec<f32>) -> usize {
        let id = self.samples.len();
        self.samples.push(data);
        id
    }

    pub fn trigger(&mut self, channel: usize, sample_id: usize) {
        if channel < 16 && sample_id < self.samples.len() {
            self.play_index[channel] = Some((sample_id, 0));
        }
    }
}

impl nullherz_traits::SignalProcessor for SamplerSidecar {
    fn process(&mut self, _in: &[&[f32]], out: &mut [&mut [f32]], _context: &mut audio_core::processors::ProcessContext) {
        for ch in 0..out.len().min(16) {
            if let Some((sample_id, mut idx)) = self.play_index[ch] {
                if sample_id >= self.samples.len() {
                    out[ch].fill(0.0);
                    self.play_index[ch] = None;
                    continue;
                }
                let sample_data = &self.samples[sample_id];
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
                if self.play_index[ch].is_some() {
                    self.play_index[ch] = Some((sample_id, idx));
                }
            } else {
                out[ch].fill(0.0);
            }
        }
    }
}

impl nullherz_traits::MidiResponder for SamplerSidecar {}

impl nullherz_traits::SnapshotProvider for SamplerSidecar {}

impl AudioProcessor for SamplerSidecar {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn apply_command(&mut self, cmd: &nullherz_traits::Command) {
        if let nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play) = cmd {
            self.trigger(0, 0);
        }
    }
}
