use nullherz_traits::{AudioProcessor, ProcessContext, Command, ProcessorMetadata, ParameterMetadata, SignalProcessor};
use std::sync::atomic::{AtomicBool, Ordering, AtomicUsize};

pub struct CaptureProcessor {
    buffer_l: Vec<f32>,
    buffer_r: Vec<f32>,
    write_ptr: AtomicUsize,
    is_recording: AtomicBool,
    has_data: AtomicBool,
    pub capture_id: u64,

    // Parameters
    input_gain: f32,
    monitor_level: f32,
    is_stereo: bool,
}

impl CaptureProcessor {
    pub fn new(capacity_samples: usize, capture_id: u64) -> Self {
        Self {
            buffer_l: vec![0.0; capacity_samples],
            buffer_r: vec![0.0; capacity_samples],
            write_ptr: AtomicUsize::new(0),
            is_recording: AtomicBool::new(false),
            has_data: AtomicBool::new(false),
            capture_id,
            input_gain: 1.0,
            monitor_level: 0.0,
            is_stereo: true,
        }
    }
}

impl nullherz_traits::SignalProcessor for CaptureProcessor {
    fn reset(&mut self) {
        self.write_ptr.store(0, Ordering::Release);
        self.is_recording.store(false, Ordering::Release);
        self.has_data.store(false, Ordering::Release);
        self.buffer_l.fill(0.0);
        self.buffer_r.fill(0.0);
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() { return; }

        let in_l = inputs[0];
        let in_r = if inputs.len() > 1 { inputs[1] } else { in_l };

        if self.is_recording.load(Ordering::Acquire) {
            let mut ptr = self.write_ptr.load(Ordering::Relaxed);
            let len = self.buffer_l.len();

            for i in 0..in_l.len() {
                let s_l = in_l[i] * self.input_gain;
                let s_r = if self.is_stereo { in_r[i] * self.input_gain } else { s_l };

                unsafe {
                    *self.buffer_l.get_unchecked_mut(ptr) = s_l;
                    *self.buffer_r.get_unchecked_mut(ptr) = s_r;
                }
                ptr = (ptr + 1) % len;
            }
            self.write_ptr.store(ptr, Ordering::Release);
            self.has_data.store(true, Ordering::Release);
        }

        // Monitoring and Passthrough
        if !outputs.is_empty() {
            for i in 0..in_l.len() {
                outputs[0][i] = in_l[i] * self.monitor_level;
                if outputs.len() > 1 {
                    outputs[1][i] = if self.is_stereo { in_r[i] } else { in_l[i] } * self.monitor_level;
                }
            }
        }
    }
}

impl nullherz_traits::MidiResponder for CaptureProcessor {
    fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { }
}

impl nullherz_traits::SnapshotProvider for CaptureProcessor {
    fn pull_snapshot(&mut self) -> Option<std::sync::Arc<Vec<f32>>> {
        if !self.has_data.load(Ordering::Acquire) {
            return None;
        }

        // For simple pull, we only support mono for now or interleaved.
        // Let's do interleaved stereo if stereo mode.
        let ptr = self.write_ptr.load(std::sync::atomic::Ordering::Acquire);
        let len = self.buffer_l.len();

        let mut snapshot = Vec::with_capacity(len * 2);
        for i in 0..len {
            let idx = (ptr + i) % len;
            snapshot.push(self.buffer_l[idx]);
            if self.is_stereo {
                snapshot.push(self.buffer_r[idx]);
            } else {
                snapshot.push(self.buffer_l[idx]);
            }
        }
        self.has_data.store(false, Ordering::Release);
        Some(std::sync::Arc::new(snapshot))
    }
}

impl AudioProcessor for CaptureProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn apply_command(&mut self, command: &Command) {
        match command {
            nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop) => {
                self.is_recording.store(false, Ordering::Release);
            }
            nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play) => {
                self.is_recording.store(true, Ordering::Release);
            }
            _ => {}
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        let value = if value.is_finite() { value } else { 0.0 };
        match param_id {
            0 => self.input_gain = value,
            1 => self.monitor_level = value,
            2 => self.is_stereo = value > 0.5,
            3 => {
                if value > 0.5 {
                    self.is_recording.store(true, Ordering::Release);
                } else {
                    self.is_recording.store(false, Ordering::Release);
                }
            }
            4
                if value > 0.5 => { self.reset(); }
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.input_gain,
            1 => self.monitor_level,
            2 => if self.is_stereo { 1.0 } else { 0.0 },
            3 => if self.is_recording.load(Ordering::Relaxed) { 1.0 } else { 0.0 },
            _ => 0.0,
        }
    }

    fn metadata(&self) -> Option<ProcessorMetadata> {
        let mut parameters = [ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 1.0,
            default: 0.0,
        }; 16];

        parameters[0] = ParameterMetadata { id: 0, name: *b"Input Gain                      ", min: 0.0, max: 4.0, default: 1.0 };
        parameters[1] = ParameterMetadata { id: 1, name: *b"Monitor Level                   ", min: 0.0, max: 1.0, default: 0.0 };
        parameters[2] = ParameterMetadata { id: 2, name: *b"Stereo Mode                     ", min: 0.0, max: 1.0, default: 1.0 };
        parameters[3] = ParameterMetadata { id: 3, name: *b"Record                          ", min: 0.0, max: 1.0, default: 0.0 };
        parameters[4] = ParameterMetadata { id: 4, name: *b"Reset                           ", min: 0.0, max: 1.0, default: 0.0 };

        Some(ProcessorMetadata {
            processor_id: self.capture_id,
            num_parameters: 5,
            parameters,
        })
    }
}
