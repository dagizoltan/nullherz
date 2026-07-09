use nullherz_traits::{AudioProcessor, SignalProcessor, ProcessContext};
use ipc_layer::ShmRingBuffer;
use std::sync::Arc;

pub struct StreamingSamplerProcessor {
    pub id: u64,
    ring_buffer: Arc<ShmRingBuffer<f32>>,
    pub playback_pos: f64,
    pub _playback_rate: f32,
    is_playing: bool,
}

impl StreamingSamplerProcessor {
    pub fn new(id: u64, ring_buffer: Arc<ShmRingBuffer<f32>>) -> Self {
        Self {
            id,
            ring_buffer,
            playback_pos: 0.0,
            _playback_rate: 1.0,
            is_playing: false,
        }
    }
}

impl SignalProcessor for StreamingSamplerProcessor {
    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if outputs.is_empty() || !self.is_playing { return; }
        let output = &mut outputs[0];
        let num_samples = output.len();

        for i in 0..num_samples {
            if let Some(sample) = self.ring_buffer.pop() {
                output[i] = sample;
            } else {
                output[i] = 0.0;
            }
        }
    }

    fn reset(&mut self) {
        self.playback_pos = 0.0;
    }
}

impl nullherz_traits::MidiResponder for StreamingSamplerProcessor { }
impl nullherz_traits::SnapshotProvider for StreamingSamplerProcessor { }

impl AudioProcessor for StreamingSamplerProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn apply_command(&mut self, command: &nullherz_traits::Command) {
        use nullherz_traits::{Command, PerformanceCommand};
        match command {
            Command::Performance(PerformanceCommand::PlayNode { .. }) => self.is_playing = true,
            Command::Performance(PerformanceCommand::StopNode { .. }) => self.is_playing = false,
            _ => {}
        }
    }
}
