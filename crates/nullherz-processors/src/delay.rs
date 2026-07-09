use nullherz_traits::{AudioProcessor, SignalProcessor, ProcessContext, AudioConfig};

use crate::{MAX_CHANNELS};

pub struct DelayProcessor {
    pub id: u64,
    buffers: [Vec<f32>; MAX_CHANNELS],
    write_pos: usize,
    delay_samples: usize,
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
            delay_samples: 0,
            capacity,
        }
    }

    pub fn set_delay(&mut self, samples: usize) {
        self.delay_samples = samples.min(self.capacity - 1);
    }
}

impl SignalProcessor for DelayProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        let num_channels = inputs.len().min(outputs.len()).min(MAX_CHANNELS);
        if num_channels == 0 { return; }

        let len = inputs[0].len();

        for i in 0..len {
            let read_pos = (self.write_pos + self.capacity - self.delay_samples) % self.capacity;

            for ch in 0..num_channels {
                self.buffers[ch][self.write_pos] = inputs[ch][i];
                outputs[ch][i] = self.buffers[ch][read_pos];
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
            self.set_delay(value as usize);
        }
    }

    fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: self.capacity as f32 - 1.0,
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
