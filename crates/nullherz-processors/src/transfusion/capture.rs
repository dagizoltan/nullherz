use nullherz_traits::{AudioProcessor, ProcessContext, Command, TopologyMutation};
use std::sync::Arc;

pub struct CaptureProcessor {
    buffer: Vec<f32>,
    write_ptr: usize,
    _is_frozen: bool,
    preallocated_capture: Vec<f32>,
    pub capture_id: u64,
    pub latest_snapshot: Option<Arc<Vec<f32>>>,
}

impl CaptureProcessor {
    pub fn new(capacity_samples: usize, capture_id: u64) -> Self {
        Self {
            buffer: vec![0.0; capacity_samples],
            write_ptr: 0,
            _is_frozen: false,
            preallocated_capture: vec![0.0; capacity_samples],
            capture_id,
            latest_snapshot: None,
        }
    }
}

impl AudioProcessor for CaptureProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn reset(&mut self) {
        self.write_ptr = 0;
        self._is_frozen = false;
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

    fn apply_command(&mut self, command: &Command) {
        match command {
            Command::Stop => {
                // To avoid allocation on RT thread, we should have a pool of Arcs.
                // For now, we perform the copy, but the user pointed out that Vec::clone and Arc::new are NOT RT-safe.
                // A better way is to have the Conductor handle the snapshotting.
                self._is_frozen = true;
            }
            Command::Play => {
                self._is_frozen = false;
            }
            _ => {}
        }
    }

    fn pull_snapshot(&mut self) -> Option<Arc<Vec<f32>>> {
        if self._is_frozen {
             let (first, second) = self.buffer.split_at(self.write_ptr);
             let mut snapshot = vec![0.0; self.buffer.len()];
             snapshot[..second.len()].copy_from_slice(second);
             snapshot[second.len()..].copy_from_slice(first);
             self._is_frozen = false;
             Some(Arc::new(snapshot))
        } else {
            None
        }
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
            processor_id: self.capture_id,
            num_parameters: 0,
            parameters,
        })
    }
}
