use nullherz_traits::{AudioProcessor, ProcessContext, Command};
use std::sync::atomic::{AtomicBool, Ordering, AtomicUsize};

pub struct CaptureProcessor {
    buffer: Vec<f32>,
    write_ptr: AtomicUsize,
    is_frozen: AtomicBool,
    pub capture_id: u64,
}

impl CaptureProcessor {
    pub fn new(capacity_samples: usize, capture_id: u64) -> Self {
        Self {
            buffer: vec![0.0; capacity_samples],
            write_ptr: AtomicUsize::new(0),
            is_frozen: AtomicBool::new(false),
            capture_id,
        }
    }
}

impl nullherz_traits::SignalProcessor for CaptureProcessor {
fn reset(&mut self) {
        self.write_ptr.store(0, Ordering::Release);
        self.is_frozen.store(false, Ordering::Release);
    }
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() { return; }
        let input = inputs[0];

        if !self.is_frozen.load(Ordering::Acquire) {
            let mut ptr = self.write_ptr.load(Ordering::Relaxed);
            let len = self.buffer.len();
            // Circular write
            for &sample in input {
                unsafe { *self.buffer.get_unchecked_mut(ptr) = sample; }
                ptr = (ptr + 1) % len;
            }
            self.write_ptr.store(ptr, Ordering::Release);
        }

        // Passthrough
        if !outputs.is_empty() {
            outputs[0].copy_from_slice(input);
        }
    }
}

impl nullherz_traits::MidiResponder for CaptureProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for CaptureProcessor {
    fn pull_snapshot(&mut self) -> Option<std::sync::Arc<Vec<f32>>> {
        if self.is_frozen.load(std::sync::atomic::Ordering::Acquire) {
            let ptr = self.write_ptr.load(std::sync::atomic::Ordering::Acquire);
            let (first, second) = self.buffer.split_at(ptr);
            let mut snapshot = Vec::with_capacity(self.buffer.len());
            snapshot.extend_from_slice(second);
            snapshot.extend_from_slice(first);
            self.is_frozen.store(false, std::sync::atomic::Ordering::Release);
            Some(std::sync::Arc::new(snapshot))
        } else { None }
    }
}

impl AudioProcessor for CaptureProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_command(&mut self, command: &Command) {
        match command {
            nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop) => {
                self.is_frozen.store(true, Ordering::Release);
            }
            nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play) => {
                self.is_frozen.store(false, Ordering::Release);
            }
            _ => {}
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
