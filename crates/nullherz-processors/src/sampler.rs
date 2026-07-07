use nullherz_traits::{AudioProcessor, SignalProcessor};
use audio_dsp::SamplerVoice;

#[derive(Debug)]
pub struct SamplerProcessor {
    pub id: u64,
    pub voices: Vec<SamplerVoice>,
    sample_buffer: std::sync::Arc<Vec<f32>>,
    render_buffer: [f32; ipc_layer::MAX_BLOCK_SIZE],
    sample_id: Option<u64>,
    metadata: Option<nullherz_traits::SampleMetadata>,
    quantize_enabled: bool,
    playback_rate: f32,
    pub slicer_mode: bool,
    pub slice_grid_beats: f32,
    pub beats_per_bar: f32,
}

impl SamplerProcessor {
    pub fn new(id: u64) -> Self {
        let voices = (0..8).map(|_| SamplerVoice::new()).collect();
        Self {
            id,
            voices,
            sample_buffer: std::sync::Arc::new(Vec::new()),
            render_buffer: [0.0; ipc_layer::MAX_BLOCK_SIZE],
            sample_id: None,
            metadata: None,
            quantize_enabled: true,
            playback_rate: 1.0,
            slicer_mode: false,
            slice_grid_beats: 0.25,
            beats_per_bar: 4.0,
        }
    }

    pub fn set_sample(&mut self, buffer: Vec<f32>) {
        self.sample_buffer = std::sync::Arc::new(buffer);
    }

    pub fn set_parameter(&mut self, param_id: u32, value: f32) {
        match param_id {
            1 => {
                self.playback_rate = value;
                for voice in self.voices.iter_mut() {
                    voice.playback_rate = value;
                }
            }
            2 => {
                self.quantize_enabled = value > 0.5;
            }
            3 => {
                self.slicer_mode = value > 0.5;
            }
            4 => {
                self.slice_grid_beats = value;
            }
            5 => {
                self.beats_per_bar = value;
            }
            _ => {}
        }
    }

    pub fn id_getter(&self) -> Option<u64> {
        self.sample_id
    }

    fn trigger_slice(&mut self, slice_idx: u32, context: Option<&nullherz_traits::ProcessContext>) {
        if let Some(voice) = self.voices.iter_mut().find(|v| !v.is_active) {
            let bpm = self.metadata.as_ref().map(|m| m.bpm).unwrap_or(120.0);
            let sample_rate = context.and_then(|c| c.transport).map(|t| t.sample_rate).unwrap_or(44100.0);
            let beat_pos = context.and_then(|c| c.transport).map(|t| t.beat_position).unwrap_or(0.0);

            let samples_per_beat = (sample_rate * 60.0 / bpm.max(1.0)) as f64;
            let offset = slice_idx as f32 * self.slice_grid_beats * samples_per_beat as f32;

            // RT-HARDENING: Use buffer_ref instead of clone to avoid atomic overhead in the hot path
            voice.trigger_at_ref(&self.sample_buffer, self.playback_rate, 1.0, offset, beat_pos);
        }
    }
}

impl nullherz_traits::SignalProcessor for SamplerProcessor {
fn reset(&mut self) {
        for v in self.voices.iter_mut() {
            v.is_active = false;
            v.play_head = 0.0;
        }
    }
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut nullherz_traits::ProcessContext) {
        self.process_parallel(inputs, outputs, context, None);
    }

fn process_parallel(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut nullherz_traits::ProcessContext, _executor: Option<&mut (dyn nullherz_traits::ParallelExecutor + '_)>) {
        // SYNC LOGIC
        if self.quantize_enabled
            && let (Some(transport), Some(meta)) = (context.transport, &self.metadata)
                && meta.bpm > 10.0 {
                    let sync_rate = (transport.bpm / meta.bpm) * self.playback_rate;
                    for voice in self.voices.iter_mut() {
                        voice.playback_rate = sync_rate;

                        // PHASE LOCK
                        if voice.is_active {
                            let samples_per_beat = (transport.sample_rate * 60.0 / meta.bpm) as f64;

                            let expected_pos_samples = if self.slicer_mode {
                                // Slicer-aware phase locking
                                let beat_diff = transport.beat_position - voice.trigger_beat;
                                voice.trigger_offset + (beat_diff as f32 * samples_per_beat as f32)
                            } else {
                                let expected_pos_beats = transport.beat_position;
                                (expected_pos_beats * samples_per_beat) as f32 % self.sample_buffer.len().max(1) as f32
                            };

                            // Gently nudge playhead towards locked phase
                            let diff = expected_pos_samples - voice.play_head;
                            if diff.abs() > 0.001 {
                                voice.play_head += diff * 0.01; // Smooth convergence
                            }
                        }
                    }
                }

        if outputs.is_empty() { return; }
        let num_samples = outputs[0].len();
        if num_samples == 0 { return; }

        // Production Hardening: Ensure we don't overflow the fixed-size render_buffer.
        // The engine should enforce this, but we protect the processor here.
        let num_samples = num_samples.min(ipc_layer::MAX_BLOCK_SIZE);

        // Render all active voices into the temporary render_buffer once per cycle.
        // Performance Optimization: Skip inactive voices to reduce branching and cache misses.
        self.render_buffer[..num_samples].fill(0.0);
        let render_slice = &mut self.render_buffer[..num_samples];
        for voice in self.voices.iter_mut().filter(|v| v.is_active) {
            voice.process_block(render_slice);
        }

        // Production Hardening: Use SIMD for multi-channel output distribution if possible
        // (Ensuring 64-byte alignment safety via audio-dsp primitives)
        use audio_dsp::simd_vec::{load_f32x8, store_f32x8};
        for output in outputs.iter_mut() {
            let mut i = 0;
            while i + 8 <= num_samples {
                let v = load_f32x8(render_slice, i);
                store_f32x8(*output, i, v);
                i += 8;
            }
            while i < num_samples {
                output[i] = render_slice[i];
                i += 1;
            }
        }
    }
}

impl nullherz_traits::MidiResponder for SamplerProcessor {
    fn apply_midi(&mut self, event: ipc_layer::MidiEvent, context: Option<&nullherz_traits::ProcessContext>) {
        let status = event.status & 0xF0;
        if status == 0x90 && event.data2 > 0 {
            if self.slicer_mode {
                if (36..=51).contains(&event.data1) {
                    self.trigger_slice((event.data1 - 36) as u32, context);
                }
            } else if let Some(voice) = self.voices.iter_mut().find(|v| !v.is_active) {
                let freq = 440.0 * 2.0f32.powf((event.data1 as f32 - 69.0) / 12.0);
                let playback_rate = (freq / 440.0) * self.playback_rate;
                let velocity = event.data2 as f32 / 127.0;
                voice.trigger(self.sample_buffer.clone(), playback_rate, velocity);
            }
        }
    }
}

impl nullherz_traits::SnapshotProvider for SamplerProcessor { }

impl AudioProcessor for SamplerProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
    self.set_parameter(param_id, value);
}

fn get_parameter(&self, param_id: u32) -> f32 {
    match param_id {
        1 => self.playback_rate,
        2 => if self.quantize_enabled { 1.0 } else { 0.0 },
        3 => if self.slicer_mode { 1.0 } else { 0.0 },
        4 => self.slice_grid_beats,
        5 => self.beats_per_bar,
        _ => 0.0,
    }
}

fn apply_topology_mutation(&mut self, mutation: nullherz_traits::TopologyMutation) {
        match mutation {
            nullherz_traits::TopologyMutation::AddSource { node_idx: _, buffer, sample_id, metadata } => {
                self.set_sample((*buffer).clone());
                self.sample_id = Some(sample_id);
                self.metadata = metadata.map(|m| (*m).clone());
            }
            nullherz_traits::TopologyMutation::UpdateMetadata { node_idx: _, metadata } => {
                self.metadata = Some((*metadata).clone());
            }
            _ => {}
        }
    }

fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        self.apply_command_with_context(command, None);
    }

    fn resource_id(&self) -> Option<u64> {
        self.sample_id
    }
}

impl SamplerProcessor {
    pub fn apply_command_with_context(&mut self, command: &nullherz_traits::ProcessorCommand, context: Option<&nullherz_traits::ProcessContext>) {
        use nullherz_traits::{Command, PerformanceCommand};
        match *command {
            Command::Performance(PerformanceCommand::JumpToHotCue { node_idx: _, cue_idx }) => {
                let offset = if let Some(ref metadata) = self.metadata {
                    metadata.hot_cues.get(cue_idx as usize).and_then(|&c| c)
                        .unwrap_or((cue_idx as f32 * 0.1 * self.sample_buffer.len() as f32) as u64)
                } else {
                    (cue_idx as f32 * 0.1 * self.sample_buffer.len() as f32) as u64
                };

                let mut offset = offset;

                // QUANTIZATION LOGIC
                if self.quantize_enabled
                    && let (Some(ctx), Some(meta)) = (context, &self.metadata)
                        && let Some(transport) = ctx.transport
                            && meta.bpm > 0.0 {
                                let samples_per_beat = (transport.sample_rate * 60.0 / meta.bpm) as f64;
                                // Align to beat grid
                                let grid_pos = (offset as f64 / samples_per_beat).round() * samples_per_beat;
                                offset = grid_pos as u64;
                            }

                for voice in self.voices.iter_mut() {
                    if voice.is_active {
                        voice.play_head = offset as f32;
                    }
                }
            }
            Command::Performance(PerformanceCommand::TriggerSlice { node_idx: _, slice_idx }) => {
                self.trigger_slice(slice_idx, context);
            }
            Command::Performance(PerformanceCommand::JumpByBeats { node_idx: _, beats }) => {
                if let (Some(ctx), Some(meta)) = (context, &self.metadata)
                    && let Some(transport) = ctx.transport
                        && meta.bpm > 0.0 {
                            let samples_per_beat = (transport.sample_rate * 60.0 / meta.bpm) as f64;
                            let jump_samples = (beats as f64 * samples_per_beat) as f32;

                            for voice in self.voices.iter_mut() {
                                if voice.is_active {
                                    // Law of Bit-Exact Reset: Jumps should be precise and not introduce DC offsets
                                    // Linear interpolation in voice handles the fractional position
                                    voice.play_head += jump_samples;

                                    // Clamp to buffer range
                                    if voice.play_head < 0.0 { voice.play_head = 0.0; }
                                    if voice.play_head >= self.sample_buffer.len() as f32 {
                                        voice.play_head = (self.sample_buffer.len() as f32 - 1.0).max(0.0);
                                    }
                                }
                            }
                        }
            }
            Command::Performance(PerformanceCommand::SetLoop { node_idx: _, enabled, start_samples, end_samples }) => {
                for voice in self.voices.iter_mut() {
                    voice.loop_enabled = enabled;
                    voice.loop_start = start_samples;
                    voice.loop_end = end_samples;
                }
            }
            Command::Performance(PerformanceCommand::PlayNode { .. }) => {
                if let Some(voice) = self.voices.iter_mut().find(|v| !v.is_active) {
                    let beat_pos = context.and_then(|c| c.transport).map(|t| t.beat_position).unwrap_or(0.0);
                    voice.trigger_at_ref(&self.sample_buffer, self.playback_rate, 1.0, 0.0, beat_pos);
                }
            }
            Command::Performance(PerformanceCommand::StopNode { .. }) => {
                self.reset();
            }
            Command::Performance(PerformanceCommand::SetSlipMode { node_idx: _, enabled }) => {
                for voice in self.voices.iter_mut() {
                    voice.slip_enabled = enabled;
                    if !enabled {
                        voice.play_head = voice.background_playhead;
                    } else {
                        voice.background_playhead = voice.play_head;
                    }
                }
            }
            _ => {}
        }
    }
}
