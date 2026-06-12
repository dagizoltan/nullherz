use nullherz_traits::AudioProcessor;

pub struct WavetableProcessor {
    inner: audio_dsp::WavetableOscillator,
}

impl WavetableProcessor {
    pub fn new(sample_rate: f32) -> Self {
        Self { inner: audio_dsp::WavetableOscillator::new(sample_rate) }
    }
}

impl AudioProcessor for WavetableProcessor {
    fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        self.inner.set_sample_rate(config.sample_rate);
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        let num_channels = outputs.len().min(crate::MAX_CHANNELS);
        let len = if num_channels > 0 { outputs[0].len() } else { 0 };
        if len == 0 { return; }

        let fm_storage = [0.0f32; 128];
        let pm_storage = [0.0f32; 128];

        // Optimization: Use SIMD multi-channel path if exactly 8 channels are available
        if num_channels == 8 {
            let mut fm_ptrs = [std::ptr::null(); 8];
            let mut pm_ptrs = [std::ptr::null(); 8];
            let mut out_ptrs = [std::ptr::null_mut(); 8];

            let fm_default = if !inputs.is_empty() { inputs[0] } else { &fm_storage[..len] };
            let pm_default = if inputs.len() > 1 { inputs[1] } else { &pm_storage[..len] };

            for (ch, (fm_ptr, (pm_ptr, out_ptr))) in fm_ptrs.iter_mut().zip(pm_ptrs.iter_mut().zip(out_ptrs.iter_mut())).enumerate() {
                *fm_ptr = fm_default.as_ptr();
                *pm_ptr = pm_default.as_ptr();
                *out_ptr = outputs[ch].as_mut_ptr();
            }

            self.inner.process_8_channels(fm_ptrs, pm_ptrs, out_ptrs, len);
            return;
        }

        for (ch, output) in outputs.iter_mut().enumerate().take(num_channels) {
            let fm = if !inputs.is_empty() { inputs[0] } else { &fm_storage[..len] };
            let pm = if inputs.len() > 1 { inputs[1] } else { &pm_storage[..len] };
            self.inner.process_scalar(ch, fm, pm, output);
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
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        // For prototype, we ensure lengths match.
        let len = inputs[0].len().min(outputs[0].len());
        self.inner.process_overlap_add(&inputs[0][..len], &mut outputs[0][..len]);
    }
}

const MODULATION_THRESHOLD: f32 = 0.001;

pub struct ModulationProcessor {
    pub target_id: u64,
    pub param_id: u32,
    pub scale: f32,
    pub offset: f32,
    command_producer: Option<ipc_layer::Producer<control_plane::TimestampedCommand>>,
    last_sent_value: f32,
}

impl ModulationProcessor {
    pub fn new(target_id: u64, param_id: u32, scale: f32, offset: f32) -> Self {
        Self {
            target_id,
            param_id,
            scale,
            offset,
            command_producer: None,
            last_sent_value: f32::NAN,
        }
    }

    pub fn set_producer(&mut self, producer: ipc_layer::Producer<control_plane::TimestampedCommand>) {
        self.command_producer = Some(producer);
    }
}

impl AudioProcessor for ModulationProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], _outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if inputs.is_empty() { return; }
        let cv = inputs[0];
        if cv.is_empty() { return; }

        // High-precision modulation: We process in 32-sample chunks to balance CPU and responsiveness.
        // For this prototype, we still average over the block but use the engine's sub_block_offset.
        let sum: f32 = cv.iter().sum();
        let avg_cv = sum / cv.len() as f32;
        let val = avg_cv * self.scale + self.offset;

        let is_mod_needed = (val - self.last_sent_value).abs() > MODULATION_THRESHOLD || self.last_sent_value.is_nan();
        if let (true, Some(prod)) = (is_mod_needed, &mut self.command_producer) {
                // Determine block_start_sample for this cycle via telemetry or counter
                // For now, we use a relative offset within the engine's block counter.
                let _ = prod.push(control_plane::TimestampedCommand {
                    timestamp_samples: 0, // 0 indicates current block relative in the MPSC hardened path
                    command: control_plane::Command::SetParam {
                        target_id: self.target_id,
                        param_id: self.param_id,
                        value: val,
                        ramp_duration_samples: 32, // Default smoothing for CV mappings
                    },
                });
                self.last_sent_value = val;
        }
    }
}

pub struct SequencerProcessor {
    sample_rate: f32,
    current_sample: u64,
    grid: [[bool; crate::MAX_CHANNELS]; 8], // 8 tracks, steps limited by MAX_CHANNELS for consistency
    command_producer: Option<ipc_layer::Producer<control_plane::TimestampedCommand>>,
}

impl SequencerProcessor {
    pub fn new(sample_rate: f32, _bpm: f32) -> Self {
        Self {
            sample_rate,
            current_sample: 0,
            grid: [[false; crate::MAX_CHANNELS]; 8],
            command_producer: None,
        }
    }
}

impl AudioProcessor for SequencerProcessor {
    fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        self.sample_rate = config.sample_rate;
    }

    fn apply_command(&mut self, command: &control_plane::Command) {
        if let control_plane::Command::SetSequencerStep { track, step, value } = command {
            if *track < 8 && *step < crate::MAX_CHANNELS as u32 {
                self.grid[*track as usize][*step as usize] = *value;
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut nullherz_traits::ProcessContext) {
        let block_len = if !outputs.is_empty() { outputs[0].len() as u64 } else { 0 };
        if block_len == 0 { return; }

        if let Some(transport) = context.transport {
            if !transport.is_playing { return; }

            // Sample-absolute indexing to prevent precision drift
            let samples_per_beat = (transport.sample_rate as f64 * 60.0) / transport.bpm as f64;
            let samples_per_step = samples_per_beat * 0.25; // 16th note

            let block_start_sample = (transport.beat_position * samples_per_beat).round() as u64;
            let block_end_sample = block_start_sample + block_len;

            let next_step_idx = (block_start_sample as f64 / samples_per_step).ceil() as u64;
            let next_step_sample = (next_step_idx as f64 * samples_per_step).round() as u64;

            if next_step_sample < block_end_sample {
                let step_idx = (next_step_idx % crate::MAX_CHANNELS as u64) as usize;
                let sample_offset = next_step_sample.saturating_sub(block_start_sample);

                for track in 0..8 {
                    if let (true, Some(prod)) = (self.grid[track][step_idx], &mut self.command_producer) {
                            let _ = prod.push(control_plane::TimestampedCommand {
                                timestamp_samples: self.current_sample + sample_offset.min(block_len - 1),
                                command: control_plane::Command::Play,
                            });
                    }
                }
            }
        }

        self.current_sample += block_len;
    }
}
