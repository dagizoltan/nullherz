use nullherz_traits::AudioProcessor;

#[derive(Clone, Copy)]
struct Pattern {
    grid: [[f32; 64]; 16], // 16 tracks, 64 steps (Velocity/Value)
    len: u32,
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            grid: [[0.0; 64]; 16],
            len: 16,
        }
    }
}

pub struct SequencerProcessor {
    pub id: u32,
    sample_rate: f32,
    current_sample: u64,
    patterns: [Pattern; 16], // 16 patterns in memory
    active_pattern: usize,
    pub quantize_amount: f32, // 0.0 to 1.0
    pub swing: f32,           // 0.0 to 1.0
    bpm: f32,
}

impl SequencerProcessor {
    pub fn new(id: u32, sample_rate: f32, bpm: f32) -> Self {
        Self {
            id,
            sample_rate,
            current_sample: 0,
            patterns: [Pattern::default(); 16],
            active_pattern: 0,
            quantize_amount: 1.0,
            swing: 0.0,
            bpm,
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

            // STAGE 9: Correct for mid-block BPM changes via transport update
            self.bpm = transport.bpm;

            // Sample-absolute indexing to prevent precision drift
            let samples_per_beat = (transport.sample_rate as f64 * 60.0) / self.bpm as f64;
            let samples_per_step = samples_per_beat * 0.25; // 16th note

            let block_start_sample = (transport.beat_position * samples_per_beat).round() as u64;
            let block_end_sample = block_start_sample + block_len;

            let next_step_idx = (block_start_sample as f64 / samples_per_step).ceil() as u64;
            let next_step_sample = (next_step_idx as f64 * samples_per_step).round() as u64;

            if next_step_sample < block_end_sample {
                let pattern = &self.patterns[self.active_pattern];

                // Real-time Quantize & Swing
                let is_even_step = next_step_idx % 2 == 0;
                let swing_offset_samples = if !is_even_step {
                    (self.swing as f64 * samples_per_step * 0.5) as u64
                } else {
                    0
                };

                let step_idx = (next_step_idx % pattern.len as u64) as usize;
                let sample_offset = next_step_sample.saturating_sub(block_start_sample) + swing_offset_samples;

                if let Some(host) = context.host {
                    for track in 0..16 {
                        let velocity = pattern.grid[track][step_idx];
                        if velocity > 0.0 {
                            // STAGE 8 Quantization Logic: Corrected to adjust timing offset
                            // quantize_amount = 1.0 (hard-locked to grid), 0.0 (unquantized)
                            let quantized_offset = 0; // Relative to step start
                            let final_offset = (sample_offset as f32 * (1.0 - self.quantize_amount) + quantized_offset as f32 * self.quantize_amount) as u64;

                            host.push_command(
                                self.current_sample + final_offset.min(block_len - 1),
                                nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: (70 + track as u64), // Placeholder targeting
                                    param_id: 0, // Gain/Velocity
                                    value: velocity,
                                    ramp_duration_samples: 0,
                                }),
                            );
                            host.push_command(
                                self.current_sample + sample_offset.min(block_len - 1),
                                nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play),
                            );
                        }
                    }
                }
            }
        }

        self.current_sample += block_len;
    }
}

impl nullherz_traits::MidiResponder for SequencerProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for SequencerProcessor { }

impl AudioProcessor for SequencerProcessor {
fn apply_command(&mut self, command: &nullherz_traits::Command) {
        #[allow(clippy::collapsible_if)]
        if let nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::SetSequencerStep { node_idx, track, step, value }) = command {
            if *node_idx == self.id && *track < 16 && *step < 64 {
                self.patterns[self.active_pattern].grid[*track as usize][*step as usize] = *value;
            }
        }

        if let nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetBpm(new_bpm)) = command {
            self.bpm = *new_bpm;
        }
    }

fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        match param_id {
            0 => { // Active Pattern
                let p = value.round() as usize;
                if p < 16 { self.active_pattern = p; }
            }
            1 => { // Pattern Length
                let l = value.round() as u32;
                if (1..=64).contains(&l) {
                    self.patterns[self.active_pattern].len = l;
                }
            }
            2 => self.quantize_amount = value.clamp(0.0, 1.0),
            3 => self.swing = value.clamp(0.0, 1.0),
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
                for step in 0..64 {
                    data.extend_from_slice(&p.grid[track][step].to_le_bytes());
                }
            }
        }
        data
    }

fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn load_state(&mut self, data: &[u8]) {
        if data.len() < 1 + 16 * (1 + 16 * 64 * 4) { return; }
        self.active_pattern = data[0] as usize;
        let mut cursor = 1;
        for p in self.patterns.iter_mut() {
            p.len = data[cursor] as u32;
            cursor += 1;
            for track in 0..16 {
                for step in 0..64 {
                    let mut b = [0u8; 4];
                    b.copy_from_slice(&data[cursor..cursor+4]);
                    p.grid[track][step] = f32::from_le_bytes(b);
                    cursor += 4;
                }
            }
        }
    }

    fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 1.0,
            default: 0.0,
        }; 16];

        let names: &[&[u8]] = &[b"ActivePattern", b"PatternLen", b"Quantize", b"Swing"];
        let mins = [0.0, 1.0, 0.0, 0.0];
        let maxs = [15.0, 64.0, 1.0, 1.0];
        let defs = [0.0, 16.0, 1.0, 0.0];

        for (i, &name) in names.iter().enumerate() {
            parameters[i].id = i as u32;
            parameters[i].name[..name.len()].copy_from_slice(name);
            parameters[i].min = mins[i];
            parameters[i].max = maxs[i];
            parameters[i].default = defs[i];
        }

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.id as u64,
            num_parameters: 4,
            parameters,
        })
    }
}
