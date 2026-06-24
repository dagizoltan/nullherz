use nullherz_traits::AudioProcessor;

#[derive(Clone, Copy)]
struct Pattern {
    grid: [[bool; 16]; 16], // 16 tracks, 16 steps
    len: u32,
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            grid: [[false; 16]; 16],
            len: 16,
        }
    }
}

pub struct SequencerProcessor {
    sample_rate: f32,
    current_sample: u64,
    patterns: [Pattern; 8], // 8 patterns in memory
    active_pattern: usize,
}

impl SequencerProcessor {
    pub fn new(sample_rate: f32, _bpm: f32) -> Self {
        Self {
            sample_rate,
            current_sample: 0,
            patterns: [Pattern::default(); 8],
            active_pattern: 0,
        }
    }
}

impl nullherz_traits::SignalProcessor for SequencerProcessor {
fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        self.sample_rate = config.sample_rate;
    }
fn reset(&mut self) {
        self.current_sample = 0;
    }
fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut nullherz_traits::ProcessContext) {
        let block_len = if !outputs.is_empty() { outputs[0].len() as u64 } else { 0 };
        if block_len == 0 { return; }

        if let Some(transport) = context.transport {
            if !transport.is_playing { return; }

            // Sample-absolute indexing to prevent precision drift
            let samples_per_beat = (transport.sample_rate as f64 * 60.0) / transport.bpm as f64;
            let samples_per_step = samples_per_beat * 0.25; // 16th note

            let block_start_sample = (transport.beat_position * samples_per_beat).round() as u64;
            let block_end_sample = block_start_sample + block_len;

            let next_step_idx = (block_start_sample as f64 / samples_per_step).ceil() as u64;
            let next_step_sample = (next_step_idx as f64 * samples_per_step).round() as u64;

            if next_step_sample < block_end_sample {
                let pattern = &self.patterns[self.active_pattern];
                let step_idx = (next_step_idx % pattern.len as u64) as usize;
                let sample_offset = next_step_sample.saturating_sub(block_start_sample);

                if let Some(host) = context.host {
                    for track in 0..16 {
                        if pattern.grid[track][step_idx] {
                            host.push_command(
                                self.current_sample + sample_offset.min(block_len - 1),
                                nullherz_traits::Command::Play,
                            );
                        }
                    }
                }
            }
        }

        self.current_sample += block_len;
    }
}

impl nullherz_traits::MidiResponder for SequencerProcessor { }

impl nullherz_traits::SnapshotProvider for SequencerProcessor { }

impl AudioProcessor for SequencerProcessor {
fn apply_command(&mut self, command: &nullherz_traits::Command) {
        #[allow(clippy::collapsible_if)]
        if let nullherz_traits::Command::SetSequencerStep { track, step, value } = command {
            if *track < 16 && *step < 16 {
                self.patterns[self.active_pattern].grid[*track as usize][*step as usize] = *value;
            }
        }
    }

fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        match param_id {
            0 => { // Active Pattern
                let p = value.round() as usize;
                if p < 8 { self.active_pattern = p; }
            }
            1 => { // Pattern Length
                let l = value.round() as u32;
                if (1..=16).contains(&l) {
                    self.patterns[self.active_pattern].len = l;
                }
            }
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.active_pattern as f32,
            1 => self.patterns[self.active_pattern].len as f32,
            _ => 0.0
        }
    }

    fn serialize_state(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(self.active_pattern as u8);
        for p in &self.patterns {
            data.push(p.len as u8);
            for track in 0..16 {
                for step in 0..16 {
                    data.push(if p.grid[track][step] { 1 } else { 0 });
                }
            }
        }
        data
    }

fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
