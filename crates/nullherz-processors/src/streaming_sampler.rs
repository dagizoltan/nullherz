use nullherz_traits::{AudioProcessor, SignalProcessor, ProcessContext, ProcessorCommand, MidiResponder, SnapshotProvider};
use ipc_layer::ShmRingBuffer;
use crate::MAX_CHANNELS;

pub struct StreamingSamplerProcessor {
    pub id: u64,
    ring_buffer: *const ShmRingBuffer<f32>,
    pub playback_pos: f64,
    pub _playback_rate: f32,
    is_playing: bool,
    volume: f32,
    pub _shm_holder: Option<Vec<u8>>,
}

impl StreamingSamplerProcessor {
    pub fn new(id: u64, ring_buffer: *const ShmRingBuffer<f32>) -> Self {
        Self {
            id,
            ring_buffer,
            playback_pos: 0.0,
            _playback_rate: 1.0,
            is_playing: false,
            volume: 1.0,
            _shm_holder: None,
        }
    }
}

impl nullherz_traits::RtSafe for StreamingSamplerProcessor {}

unsafe impl Send for StreamingSamplerProcessor {}
unsafe impl Sync for StreamingSamplerProcessor {}

impl SignalProcessor for StreamingSamplerProcessor {
    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if outputs.is_empty() { return; }
        let num_samples = outputs[0].len();
        let num_channels = outputs.len().min(MAX_CHANNELS);

        if !self.is_playing || self.ring_buffer.is_null() {
            for ch in 0..num_channels {
                outputs[ch][..num_samples].fill(0.0);
            }
            return;
        }

        for i in 0..num_samples {
            let sample_opt = unsafe { (*self.ring_buffer).pop() };
            if let Some(sample) = sample_opt {
                let scaled_sample = sample * self.volume;
                for ch in 0..num_channels {
                    outputs[ch][i] = scaled_sample;
                }
            } else {
                for ch in 0..num_channels {
                    outputs[ch][i] = 0.0;
                }
            }
        }
    }

    fn reset(&mut self) {
        self.playback_pos = 0.0;
    }
}

impl MidiResponder for StreamingSamplerProcessor {}
impl SnapshotProvider for StreamingSamplerProcessor {}

impl AudioProcessor for StreamingSamplerProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        use nullherz_traits::{Command, PerformanceCommand};
        match command {
            Command::Performance(PerformanceCommand::PlayNode { node_idx }) => {
                if *node_idx as u64 == self.id {
                    self.is_playing = true;
                }
            }
            Command::Performance(PerformanceCommand::StopNode { node_idx }) => {
                if *node_idx as u64 == self.id {
                    self.is_playing = false;
                }
            }
            Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, .. }) => {
                if *target_id == self.id {
                    self.set_parameter(*param_id, *value, 0);
                }
            }
            _ => {}
        }
    }

    fn set_parameter(&mut self, param_id: u32, mut value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() { value = 1.0; }
        if param_id == 0 {
            self.volume = value.clamp(0.0, 4.0);
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        if param_id == 0 {
            self.volume
        } else {
            0.0
        }
    }

    fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 4.0,
            default: 1.0,
        }; 16];

        let name = b"Volume";
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
    use nullherz_traits::{ProcessContext, Command};

    #[test]
    fn test_streaming_sampler_playback_stereo() {
        // Build raw memory backing for ShmRingBuffer
        let capacity = 32;
        let (layout, _) = ShmRingBuffer::<f32>::layout(capacity);
        let mut mem = vec![0u8; layout.size() + 64];
        let aligned_ptr = unsafe { mem.as_mut_ptr().add(mem.as_mut_ptr().align_offset(64)) };
        let rb_ptr = unsafe { ShmRingBuffer::<f32>::init(aligned_ptr, capacity) };

        // Push test samples into ring buffer
        unsafe {
            (*rb_ptr).push(0.5).unwrap();
            (*rb_ptr).push(-0.25).unwrap();
        }

        let mut sampler = StreamingSamplerProcessor::new(1, rb_ptr);
        sampler._shm_holder = Some(mem);
        sampler.apply_command(&Command::Performance(nullherz_traits::PerformanceCommand::PlayNode { node_idx: 1 }));

        let mut out_l = vec![0.0; 2];
        let mut out_r = vec![0.0; 2];
        let mut out_l_ref = &mut out_l[..];
        let mut out_r_ref = &mut out_r[..];
        let outputs: &mut [&mut [f32]] = &mut [&mut out_l_ref, &mut out_r_ref];

        let mut context = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: false,
        };

        sampler.process(&[], outputs, &mut context);

        // Verify correct stereo distribution and gain scaling
        assert_eq!(out_l[0], 0.5);
        assert_eq!(out_r[0], 0.5);
        assert_eq!(out_l[1], -0.25);
        assert_eq!(out_r[1], -0.25);
    }
}
