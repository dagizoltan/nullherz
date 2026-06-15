use nullherz_traits::AudioProcessor;

pub struct SequencerProcessor {
    sample_rate: f32,
    current_sample: u64,
    grid: [[bool; crate::MAX_CHANNELS]; 8], // 8 tracks, steps limited by MAX_CHANNELS for consistency
    command_producer: Option<ipc_layer::Producer<control_plane::TimestampedCommand>>,
}

impl SequencerProcessor {
    pub fn new(sample_rate: f32, _bpm: f32) -> Self {
        Self {
            sample_rate,
            current_sample: 0,
            grid: [[false; crate::MAX_CHANNELS]; 8],
            command_producer: None,
        }
    }
}

impl AudioProcessor for SequencerProcessor {
    fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        self.sample_rate = config.sample_rate;
    }

    fn apply_command(&mut self, command: &control_plane::Command) {
        #[allow(clippy::collapsible_if)]
        if let control_plane::Command::SetSequencerStep { track, step, value } = command {
            if *track < 8 && *step < crate::MAX_CHANNELS as u32 {
                self.grid[*track as usize][*step as usize] = *value;
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

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
                let step_idx = (next_step_idx % crate::MAX_CHANNELS as u64) as usize;
                let sample_offset = next_step_sample.saturating_sub(block_start_sample);

                for track in 0..8 {
                    if let (true, Some(prod)) = (self.grid[track][step_idx], &mut self.command_producer) {
                            let _ = prod.push(control_plane::TimestampedCommand {
                                timestamp_samples: self.current_sample + sample_offset.min(block_len - 1),
                                command: control_plane::Command::Play,
                            });
                    }
                }
            }
        }

        self.current_sample += block_len;
    }
}
