use std::sync::Arc;
use nullherz_traits::{AudioProcessor, ProcessContext};

pub struct CaptureProcessor {
    buffer: Vec<f32>,
    write_ptr: usize,
    _is_frozen: bool,
    _captured_arc: Option<Arc<Vec<f32>>>,
}

impl CaptureProcessor {
    pub fn new(capacity_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; capacity_samples],
            write_ptr: 0,
            _is_frozen: false,
            _captured_arc: None,
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
                let mut captured = vec![0.0; self.buffer.len()];
                // Correct chronological order: oldest data starts at write_ptr
                let (first, second) = self.buffer.split_at(self.write_ptr);
                // second is [write_ptr..len] (oldest)
                // first is [0..write_ptr] (newest)
                captured[..second.len()].copy_from_slice(second);
                captured[second.len()..].copy_from_slice(first);
                self._captured_arc = Some(Arc::new(captured));
                self._is_frozen = true;
            }
            nullherz_traits::Command::Play => {
                self._is_frozen = false;
            }
            _ => {}
        }
    }
}
