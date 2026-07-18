use nullherz_traits::{AudioProcessor, SignalProcessor, ProcessContext, AudioConfig, ProcessorCommand, Command, MidiEvent, MidiResponder, SnapshotProvider};
use crate::MAX_CHANNELS;

pub struct LimiterProcessor {
    pub id: u64,
    buffers: [Vec<f32>; MAX_CHANNELS],
    peak_buffer: Vec<f32>,
    write_pos: usize,
    sample_rate: f32,

    // Parameters
    threshold: f32,
    release_ms: f32,
    lookahead_ms: f32,
    ceiling: f32,

    // Calculated state
    release_coef: f32,
    lookahead_samples: usize,
    envelope: f32,
    capacity: usize,
}

impl LimiterProcessor {
    pub fn new(id: u64, sample_rate: f32) -> Self {
        let capacity = 2048; // Safe upper bound for look-ahead
        let buffers = std::array::from_fn(|_| vec![0.0; capacity]);
        let peak_buffer = vec![0.0; capacity];

        let mut processor = Self {
            id,
            buffers,
            peak_buffer,
            write_pos: 0,
            sample_rate,
            threshold: 1.0,
            release_ms: 100.0,
            lookahead_ms: 2.0,
            ceiling: 1.0,
            release_coef: 0.0,
            lookahead_samples: 0,
            envelope: 0.0,
            capacity,
        };
        processor.update_derived_params();
        processor
    }

    fn update_derived_params(&mut self) {
        // Look-ahead samples calculation
        let lookahead_samples = (self.lookahead_ms * 0.001 * self.sample_rate).round() as usize;
        self.lookahead_samples = lookahead_samples.clamp(1, self.capacity - 10);

        // Exponential release coefficient
        let release_samples = self.release_ms * 0.001 * self.sample_rate;
        self.release_coef = (-1.0 / release_samples.max(1.0)).exp();
    }
}

impl nullherz_traits::RtSafe for LimiterProcessor {}

impl SignalProcessor for LimiterProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        let num_channels = inputs.len().min(outputs.len()).min(MAX_CHANNELS);
        if num_channels == 0 { return; }

        let len = inputs[0].len();
        let threshold = self.threshold;
        let ceiling = self.ceiling;

        for i in 0..len {
            // Find current sample multi-channel absolute peak
            let mut current_peak = 1e-6_f32;
            for ch in 0..num_channels {
                current_peak = current_peak.max(inputs[ch][i].abs());
                self.buffers[ch][self.write_pos] = inputs[ch][i];
            }
            self.peak_buffer[self.write_pos] = current_peak;

            // Look-ahead peak detection: find the maximum peak over the look-ahead window
            let mut window_max = 0.0_f32;
            for offset in 0..self.lookahead_samples {
                let idx = (self.write_pos + self.capacity - offset) % self.capacity;
                window_max = window_max.max(self.peak_buffer[idx]);
            }

            // Smooth the peak envelope using release ballistics
            self.envelope = window_max.max(self.envelope * self.release_coef);

            // Compute the target gain reduction to brickwall limit the signal
            let gain = if self.envelope > threshold {
                threshold / self.envelope
            } else {
                1.0
            };

            // Calculate output multiplier (including makeup gain to ceiling)
            let output_scale = gain * (ceiling / threshold);

            // Retrieve the delayed audio signal from the look-ahead delay line and apply gain
            let read_pos = (self.write_pos + self.capacity - self.lookahead_samples) % self.capacity;
            for ch in 0..num_channels {
                outputs[ch][i] = self.buffers[ch][read_pos] * output_scale;
            }

            self.write_pos = (self.write_pos + 1) % self.capacity;
        }
    }

    fn setup(&mut self, config: AudioConfig) {
        self.sample_rate = config.sample_rate;
        self.reset();
    }

    fn reset(&mut self) {
        for b in self.buffers.iter_mut() { b.fill(0.0); }
        self.peak_buffer.fill(0.0);
        self.write_pos = 0;
        self.envelope = 0.0;
        self.update_derived_params();
    }
}

impl MidiResponder for LimiterProcessor {
    fn apply_midi(&mut self, _event: MidiEvent, _context: Option<&ProcessContext>) {}
}

impl SnapshotProvider for LimiterProcessor {}

impl AudioProcessor for LimiterProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, .. }) = command
            && *target_id == self.id {
                self.set_parameter(*param_id, *value, 0);
            }
    }

    fn set_parameter(&mut self, param_id: u32, mut value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() { value = 0.0; }
        match param_id {
            0 => self.threshold = value.clamp(0.001, 1.0),
            1 => self.release_ms = value.clamp(1.0, 1000.0),
            2 => self.lookahead_ms = value.clamp(0.1, 5.0),
            3 => self.ceiling = value.clamp(0.01, 1.0),
            _ => {}
        }
        self.update_derived_params();
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.threshold,
            1 => self.release_ms,
            2 => self.lookahead_ms,
            3 => self.ceiling,
            _ => 0.0,
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

        let names: &[&[u8]] = &[b"Threshold", b"Release", b"Lookahead", b"Ceiling"];
        let mins = [0.001, 1.0, 0.1, 0.01];
        let maxs = [1.0, 1000.0, 5.0, 1.0];
        let defs = [1.0, 100.0, 2.0, 1.0];

        for (i, &name) in names.iter().enumerate() {
            parameters[i].id = i as u32;
            parameters[i].name[..name.len()].copy_from_slice(name);
            parameters[i].min = mins[i];
            parameters[i].max = maxs[i];
            parameters[i].default = defs[i];
        }

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 4,
            parameters,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::ProcessContext;

    #[test]
    fn test_limiter_brickwall_containment() {
        let mut limiter = LimiterProcessor::new(1, 44100.0);
        limiter.set_parameter(0, 0.5, 0); // threshold = 0.5
        limiter.set_parameter(3, 0.5, 0); // ceiling = 0.5

        let input_left = vec![2.0, 1.5, -3.0, 0.0, 0.1];
        let input_right = vec![0.5, -2.0, 1.0, -1.0, 0.2];
        let mut out_left = vec![0.0; 5];
        let mut out_right = vec![0.0; 5];

        let mut context = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: false,
        };

        // Process a block
        {
            let inputs: &[&[f32]] = &[&input_left, &input_right];
            let mut out_l_ref = &mut out_left[..];
            let mut out_r_ref = &mut out_right[..];
            let outputs: &mut [&mut [f32]] = &mut [&mut out_l_ref, &mut out_r_ref];
            limiter.process(inputs, outputs, &mut context);
        }

        // Verify outputs are brickwalled strictly within the ceiling (with 1e-4 tolerance for float margin)
        for val in out_left {
            assert!(val.abs() <= 0.5001, "Left channel exceeded ceiling: {}", val);
        }
        for val in out_right {
            assert!(val.abs() <= 0.5001, "Right channel exceeded ceiling: {}", val);
        }
    }
}
