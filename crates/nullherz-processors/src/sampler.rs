use nullherz_traits::AudioProcessor;
use audio_dsp::SamplerVoice;

#[derive(Debug)]
pub struct SamplerProcessor {
    pub id: u64,
    voices: Vec<SamplerVoice>,
    sample_buffer: std::sync::Arc<Vec<f32>>,
    render_buffer: [f32; ipc_layer::MAX_BLOCK_SIZE],
    sample_id: Option<u64>,
    metadata: Option<nullherz_traits::SampleMetadata>,
    quantize_enabled: bool,
    playback_rate: f32,
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
            _ => {}
        }
    }

    pub fn id_getter(&self) -> Option<u64> {
        self.sample_id
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
                            let expected_pos_beats = transport.beat_position;
                            let expected_pos_samples = (expected_pos_beats * samples_per_beat) % self.sample_buffer.len().max(1) as f64;

                            // Gently nudge playhead towards locked phase
                            let diff = expected_pos_samples as f32 - voice.play_head;
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

        // Render all voices into the temporary render_buffer once per cycle.
        self.render_buffer[..num_samples].fill(0.0);
        let render_slice = &mut self.render_buffer[..num_samples];
        for voice in self.voices.iter_mut() {
            voice.process_block(render_slice);
        }

        // Copy the rendered result to all output channels.
        for output in outputs.iter_mut() {
            output.copy_from_slice(render_slice);
        }
    }
}

impl nullherz_traits::MidiResponder for SamplerProcessor {
    fn apply_midi(&mut self, event: ipc_layer::MidiEvent) {
        let status = event.status & 0xF0;
        if status == 0x90 && event.data2 > 0
            && let Some(voice) = self.voices.iter_mut().find(|v| !v.is_active) {
                let freq = 440.0 * 2.0f32.powf((event.data1 as f32 - 69.0) / 12.0);
                let playback_rate = (freq / 440.0) * self.playback_rate;
                let velocity = event.data2 as f32 / 127.0;
                voice.trigger(self.sample_buffer.clone(), playback_rate, velocity);
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
    fn apply_command_with_context(&mut self, command: &nullherz_traits::ProcessorCommand, context: Option<&nullherz_traits::ProcessContext>) {
        match *command {
            nullherz_traits::Command::JumpToHotCue { node_idx: _, cue_idx } => {
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
                                let current_beat = transport.beat_position;
                                let next_beat = current_beat.ceil();
                                let beats_to_wait = next_beat - current_beat;
                                let _samples_to_wait = (beats_to_wait * samples_per_beat) as u64;

                                // Shift offset to align with grid if we were to jump NOW
                                // Actually, for DJ sync, we usually want to delay the jump until the next beat
                                // but for simplicity in this kernel, we'll align the target offset.
                                let grid_pos = (offset as f64 / samples_per_beat).round() * samples_per_beat;
                                offset = grid_pos as u64;
                            }

                for voice in self.voices.iter_mut() {
                    if voice.is_active {
                        voice.play_head = offset as f32;
                    }
                }
            }
            nullherz_traits::Command::SetLoop { node_idx: _, enabled, start_samples, end_samples } => {
                for voice in self.voices.iter_mut() {
                    voice.loop_enabled = enabled;
                    voice.loop_start = start_samples;
                    voice.loop_end = end_samples;
                }
            }
            nullherz_traits::Command::SetSlipMode { node_idx: _, enabled } => {
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
