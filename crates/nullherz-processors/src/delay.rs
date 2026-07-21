use nullherz_traits::{AudioProcessor, SignalProcessor, ProcessContext, AudioConfig};

use crate::{MAX_CHANNELS};

pub struct DelayProcessor {
    pub id: u64,
    buffers: [Vec<f32>; MAX_CHANNELS],
    write_pos: usize,
    delay_samples: f32,
    capacity: usize,
}

impl DelayProcessor {
    pub fn new(id: u64, max_delay: usize) -> Self {
        let capacity = max_delay.next_power_of_two().max(1024);
        let buffers = std::array::from_fn(|_| vec![0.0; capacity]);
        Self {
            id,
            buffers,
            write_pos: 0,
            delay_samples: 0.0,
            capacity,
        }
    }

    pub fn set_delay(&mut self, samples: f32) {
        // Clamp to ensure we have at least 1 sample of look-behind and 2 samples of look-ahead
        // within the ring buffer capacity limits.
        self.delay_samples = samples.clamp(0.0, self.capacity as f32 - 3.0);
    }
}

impl SignalProcessor for DelayProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        let num_channels = inputs.len().min(outputs.len()).min(MAX_CHANNELS);
        if num_channels == 0 { return; }

        let len = inputs[0].len();
        let integer_delay = self.delay_samples.floor() as usize;
        let fraction = self.delay_samples - self.delay_samples.floor();

        for i in 0..len {
            // idx_0 = the sample `integer_delay` ago. The fractional taps must
            // straddle it toward MORE delay: p1 (integer) and p2 (integer+1),
            // with p0/p3 one further on each side, so a larger `fraction` reads
            // FURTHER BACK. The previous layout used (integer-1, integer,
            // integer+1, integer-2) — taps toward LESS delay — which inverted
            // the fractional part: a requested 4.5-sample delay resolved to
            // ~3.5, and sweeping the delay time sawtoothed instead of gliding.
            let idx_0 = (self.write_pos + self.capacity - integer_delay) % self.capacity;
            let idx_newer = (idx_0 + 1) % self.capacity;                    // integer-1 ago
            let idx_older = (idx_0 + self.capacity - 1) % self.capacity;    // integer+1 ago
            let idx_older2 = (idx_0 + self.capacity - 2) % self.capacity;   // integer+2 ago

            for ch in 0..num_channels {
                self.buffers[ch][self.write_pos] = inputs[ch][i];

                let p0 = self.buffers[ch][idx_newer];  // integer-1 ago
                let p1 = self.buffers[ch][idx_0];      // integer ago
                let p2 = self.buffers[ch][idx_older];  // integer+1 ago
                let p3 = self.buffers[ch][idx_older2]; // integer+2 ago

                // 4-point (3rd-order) Hermite: interpolate p1 -> p2 by `fraction`.
                let c0 = p1;
                let c1 = 0.5 * (p2 - p0);
                let c2 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
                let c3 = 0.5 * (p3 - p0) + 1.5 * (p1 - p2);

                let out = ((c3 * fraction + c2) * fraction + c1) * fraction + c0;
                outputs[ch][i] = out;
            }

            self.write_pos = (self.write_pos + 1) % self.capacity;
        }
    }

    fn setup(&mut self, _config: AudioConfig) {
        self.reset();
    }

    fn reset(&mut self) {
        for b in self.buffers.iter_mut() { b.fill(0.0); }
        self.write_pos = 0;
    }
}

impl nullherz_traits::MidiResponder for DelayProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&ProcessContext>) { } }
impl nullherz_traits::SnapshotProvider for DelayProcessor { }

impl AudioProcessor for DelayProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp: u32) {
        if param_id == 0 {
            self.set_delay(value);
        }
    }

    fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: self.capacity as f32 - 3.0,
            default: 0.0,
        }; 16];

        let name = b"DelaySamples";
        parameters[0].name[..name.len()].copy_from_slice(name);

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 1,
            parameters,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The effective delay must equal the requested delay, integer OR
    /// fractional. Drive an impulse and check the output's center of mass. The
    /// old tap layout inverted the fractional part (a 4.5-sample request landed
    /// at ~3.5 and sweeping the time sawtoothed).
    fn impulse_center_of_mass(delay: f32) -> f32 {
        let mut d = DelayProcessor::new(1, 4096);
        d.set_delay(delay);
        let mut ctx = ProcessContext { transport: None, host: None, sub_block_offset: 0, is_last_sub_block: false };
        let mut input = vec![0.0f32; 32];
        input[0] = 1.0;
        let mut out = vec![0.0f32; 32];
        {
            let inputs: &[&[f32]] = &[&input];
            let mut o = &mut out[..];
            let outputs: &mut [&mut [f32]] = &mut [&mut o];
            d.process(inputs, outputs, &mut ctx);
        }
        let sum: f32 = out.iter().sum();
        assert!(sum.abs() > 1e-6, "delay {} produced ~silence", delay);
        out.iter().enumerate().map(|(i, &v)| i as f32 * v).sum::<f32>() / sum
    }

    #[test]
    fn test_delay_effective_time_matches_request() {
        for &d in &[4.0f32, 4.25, 4.5, 4.75, 5.0, 8.0, 8.5] {
            let com = impulse_center_of_mass(d);
            assert!(
                (com - d).abs() < 0.05,
                "delay {}: impulse center of mass {:.3}, expected ~{}",
                d, com, d
            );
        }
    }
}
