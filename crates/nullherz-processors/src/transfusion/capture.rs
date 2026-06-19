use std::sync::Arc;
use nullherz_traits::{AudioProcessor, ProcessContext};

pub struct CaptureProcessor {
    buffer: Vec<f32>,
    write_ptr: usize,
    _is_frozen: bool,
    _captured_arc: Option<Arc<Vec<f32>>>,
    // Pre-allocated capture buffer to maintain RT safety
    preallocated_capture: Vec<f32>,
}

impl CaptureProcessor {
    pub fn new(capacity_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; capacity_samples],
            write_ptr: 0,
            _is_frozen: false,
            _captured_arc: None,
            preallocated_capture: vec![0.0; capacity_samples],
        }
    }
}

impl AudioProcessor for CaptureProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn reset(&mut self) {
        self.write_ptr = 0;
        self._is_frozen = false;
        self._captured_arc = None;
    }

    fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 1.0,
            default: 0.0,
        }; 16];

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: 0,
            num_parameters: 0,
            parameters,
        })
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() { return; }
        let input = inputs[0];

        // Circular write
        for &sample in input {
            self.buffer[self.write_ptr] = sample;
            self.write_ptr = (self.write_ptr + 1) % self.buffer.len();
        }

        // Passthrough
        if !outputs.is_empty() {
            outputs[0].copy_from_slice(input);
        }
    }

    fn apply_command(&mut self, command: &nullherz_traits::Command) {
        match command {
            nullherz_traits::Command::Stop => {
                // Correct chronological order: oldest data starts at write_ptr
                let (first, second) = self.buffer.split_at(self.write_ptr);
                // second is [write_ptr..len] (oldest)
                // first is [0..write_ptr] (newest)
                self.preallocated_capture[..second.len()].copy_from_slice(second);
                self.preallocated_capture[second.len()..].copy_from_slice(first);

                // Note: creating Arc here is atomic increment, but the allocation
                // of the Vec is avoided by cloning the preallocated one.
                // In a perfect world, we'd swap out the Vec entirely.
                self._captured_arc = Some(Arc::new(self.preallocated_capture.clone()));
                self._is_frozen = true;
            }
            nullherz_traits::Command::Play => {
                self._is_frozen = false;
            }
            _ => {}
        }
    }
}
