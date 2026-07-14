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
            // idx_0 is the base read index
            let idx_0 = (self.write_pos + self.capacity - integer_delay) % self.capacity;
            let idx_m1 = (idx_0 + self.capacity - 1) % self.capacity;
            let idx_1 = (idx_0 + 1) % self.capacity;
            let idx_2 = (idx_0 + 2) % self.capacity;

            for ch in 0..num_channels {
                self.buffers[ch][self.write_pos] = inputs[ch][i];

                let ym1 = self.buffers[ch][idx_m1];
                let y0  = self.buffers[ch][idx_0];
                let y1  = self.buffers[ch][idx_1];
                let y2  = self.buffers[ch][idx_2];

                // 3rd-order Hermite spline interpolation
                let c0 = y0;
                let c1 = 0.5 * (y1 - ym1);
                let c2 = ym1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
                let c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1);

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
