use crate::processors::AudioProcessor;

pub struct WavetableProcessor {
    inner: audio_dsp::WavetableOscillator,
}

impl WavetableProcessor {
    pub fn new(sample_rate: f32) -> Self {
        Self { inner: audio_dsp::WavetableOscillator::new(sample_rate) }
    }
}

impl AudioProcessor for WavetableProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        let num_channels = outputs.len().min(crate::MAX_CHANNELS);
        let len = if num_channels > 0 { outputs[0].len() } else { 0 };
        if len == 0 { return; }

        let fm_storage = [0.0f32; 128];
        let pm_storage = [0.0f32; 128];

        for ch in 0..num_channels {
            let fm = if inputs.len() > 0 { inputs[0] } else { &fm_storage[..len] };
            let pm = if inputs.len() > 1 { inputs[1] } else { &pm_storage[..len] };
            self.inner.process_scalar(ch, fm, pm, outputs[ch]);
        }
    }
}

pub struct SpectralProcessor {
    inner: audio_dsp::SpectralProcessor,
}

impl SpectralProcessor {
    pub fn new(fft_size: usize) -> Self {
        Self { inner: audio_dsp::SpectralProcessor::new(fft_size) }
    }
}

impl AudioProcessor for SpectralProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        // For prototype, we ensure lengths match.
        let len = inputs[0].len().min(outputs[0].len());
        self.inner.process_overlap_add(&inputs[0][..len], &mut outputs[0][..len]);
    }
}

pub struct ModulationProcessor {
    pub target_id: u64,
    pub param_id: u32,
    pub scale: f32,
    pub offset: f32,
    command_producer: Option<ipc_layer::Producer<control_plane::TimestampedCommand>>,
}

impl ModulationProcessor {
    pub fn new(target_id: u64, param_id: u32, scale: f32, offset: f32) -> Self {
        Self { target_id, param_id, scale, offset, command_producer: None }
    }

    pub fn set_producer(&mut self, producer: ipc_layer::Producer<control_plane::TimestampedCommand>) {
        self.command_producer = Some(producer);
    }
}

impl AudioProcessor for ModulationProcessor {
    fn process(&mut self, inputs: &[&[f32]], _outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() { return; }
        let cv = inputs[0];
        if cv.is_empty() { return; }

        let avg_cv: f32 = cv.iter().sum::<f32>() / cv.len() as f32;
        let val = avg_cv * self.scale + self.offset;

        // In a real system, we'd send a command back to the control plane
        // or directly to the target processor if it's in the same graph.
        // For now, this serves as a placeholder for CV-to-Parameter mapping.
    }
}

pub struct SequencerProcessor {
    bpm: f32,
    sample_rate: f32,
    current_sample: u64,
    grid: [[bool; 16]; 8], // 8 tracks, 16 steps
    command_producer: Option<ipc_layer::Producer<control_plane::TimestampedCommand>>,
}

impl SequencerProcessor {
    pub fn new(sample_rate: f32, bpm: f32) -> Self {
        Self {
            bpm,
            sample_rate,
            current_sample: 0,
            grid: [[false; 16]; 8],
            command_producer: None,
        }
    }
}

impl AudioProcessor for SequencerProcessor {
    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        let block_len = if !outputs.is_empty() { outputs[0].len() as u64 } else { 0 };
        if block_len == 0 { return; }

        let samples_per_step = (self.sample_rate * 60.0 / self.bpm / 4.0) as u64;

        let step_before = self.current_sample / samples_per_step;
        let step_after = (self.current_sample + block_len) / samples_per_step;

        if step_after > step_before {
            let active_step = (step_after % 16) as usize;
            for track in 0..8 {
                if self.grid[track][active_step] {
                    if let Some(ref mut prod) = self.command_producer {
                        let _ = prod.push(control_plane::TimestampedCommand {
                            timestamp_samples: self.current_sample + (samples_per_step - (self.current_sample % samples_per_step)),
                            command: control_plane::Command::Play,
                        });
                    }
                }
            }
        }

        self.current_sample += block_len;
    }
}
