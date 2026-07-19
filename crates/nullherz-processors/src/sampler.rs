use nullherz_traits::AudioProcessor;
use audio_dsp::SamplerVoice;

#[derive(Debug)]
pub struct SamplerProcessor {
    pub id: u64,
    pub voices: Vec<SamplerVoice>,
    sample_buffer: std::sync::Arc<Vec<f32>>,
    sample_id: Option<u64>,
    metadata: Option<std::sync::Arc<nullherz_traits::SampleMetadata>>,
    quantize_enabled: bool,
    playback_rate: f32,
    /// PlayNode arrived before the sample buffer (command-bus ordering:
    /// AddSource rides the sample-accurate queue, PlayNode the bundle bus).
    /// Fire the trigger as soon as the source lands.
    pending_play: bool,
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
            sample_id: None,
            metadata: None,
            quantize_enabled: true,
            playback_rate: 1.0,
            pending_play: false,
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

    /// Channel count of the loaded sample (1 when unknown).
    fn source_channels(&self) -> usize {
        self.metadata.as_ref().map(|m| m.channels as usize).unwrap_or(1).max(1)
    }

    /// Frames PER CHANNEL of the loaded sample. Metadata is authoritative;
    /// dividing the buffer length by the channel count is the fallback for
    /// sources registered without it.
    fn source_frames(&self) -> usize {
        let channels = self.source_channels();
        match self.metadata.as_ref().map(|m| m.total_samples as usize) {
            Some(frames) if frames > 0 && frames * channels <= self.sample_buffer.len() => frames,
            _ => self.sample_buffer.len() / channels,
        }
    }

    /// Tell a freshly triggered voice how to read the planar buffer.
    fn apply_layout(voice: &mut SamplerVoice, frames: usize, channels: usize) {
        voice.set_layout(frames, channels);
    }

    fn trigger_slice(&mut self, slice_idx: u32, context: Option<&nullherz_traits::ProcessContext>) {
        let (frames, channels) = (self.source_frames(), self.source_channels());
        if let Some(voice) = self.voices.iter_mut().find(|v| !v.is_active) {
            let bpm = self.metadata.as_ref().map(|m| m.bpm).unwrap_or(120.0);
            let sample_rate = context.and_then(|c| c.transport).map(|t| t.sample_rate).unwrap_or(44100.0);
            let beat_pos = context.and_then(|c| c.transport).map(|t| t.beat_position).unwrap_or(0.0);

            let samples_per_beat = (sample_rate * 60.0 / bpm.max(1.0)) as f64;
            let offset = slice_idx as f32 * self.slice_grid_beats * samples_per_beat as f32;

            // RT-HARDENING: Use buffer_ref instead of clone to avoid atomic overhead in the hot path
            voice.trigger_at_ref(&self.sample_buffer, self.playback_rate, 1.0, offset, beat_pos);
            Self::apply_layout(voice, frames, channels);
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
        let source_frames = self.source_frames();
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
                                (expected_pos_beats * samples_per_beat) as f32 % source_frames.max(1) as f32
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

        // Voices accumulate, so the outputs must start clean.
        for output in outputs.iter_mut() {
            output[..num_samples].fill(0.0);
        }

        // Render every active voice straight into the real outputs, one plane
        // per channel. The previous code rendered a single MONO buffer and
        // copied it to every output, which threw away the right channel of any
        // stereo source before the strip ever saw it. Writing into `outputs`
        // directly also keeps this allocation-free on the audio thread.
        for voice in self.voices.iter_mut().filter(|v| v.is_active) {
            voice.process_block_planar(outputs, num_samples);
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
            } else {
                let (frames, channels) = (self.source_frames(), self.source_channels());
                if let Some(voice) = self.voices.iter_mut().find(|v| !v.is_active) {
                    let freq = 440.0 * 2.0f32.powf((event.data1 as f32 - 69.0) / 12.0);
                    let playback_rate = (freq / 440.0) * self.playback_rate;
                    let velocity = event.data2 as f32 / 127.0;
                    voice.trigger(self.sample_buffer.clone(), playback_rate, velocity);
                    Self::apply_layout(voice, frames, channels);
                }
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
                // RT path: adopt the shared buffers — deep-cloning a full track
                // here (malloc + tens-of-MB memcpy on the audio thread) caused
                // multi-ms block spikes at deck load.
                //
                // A new source invalidates EVERY existing voice: each voice
                // holds its own Arc to the audio it was triggered with, so
                // without this a playing voice kept sounding the PREVIOUS
                // track after a load, and a paused one would resume it.
                // (Dropping the old Arcs here is a refcount decrement; the
                // registry retains the buffers.)
                for v in self.voices.iter_mut() {
                    v.is_active = false;
                    v.play_head = 0.0;
                    v.buffer = None;
                }
                self.sample_buffer = buffer;
                self.sample_id = Some(sample_id);
                self.metadata = metadata;
                if self.pending_play && !self.sample_buffer.is_empty() {
                    self.pending_play = false;
                    let (frames, channels) = (self.source_frames(), self.source_channels());
                    if let Some(voice) = self.voices.iter_mut().find(|v| !v.is_active) {
                        voice.trigger_at_ref(&self.sample_buffer, self.playback_rate, 1.0, 0.0, 0.0);
                        Self::apply_layout(voice, frames, channels);
                    }
                }
            }
            nullherz_traits::TopologyMutation::UpdateMetadata { node_idx: _, metadata } => {
                self.metadata = Some(metadata);
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

    fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 2.0,
            default: 1.0,
        }; 16];

        let names: &[&[u8]] = &[b"PlaybackRate", b"Quantize", b"SlicerMode", b"GridBeats", b"BeatsPerBar"];
        for (i, &name) in names.iter().enumerate() {
            parameters[i].id = (i + 1) as u32;
            parameters[i].name[..name.len()].copy_from_slice(name);
        }

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 5,
            parameters,
        })
    }

    fn processor_type(&self) -> &'static str {
        "sampler"
    }

    fn get_playback_position(&self) -> u64 {
        // Active voice wins; otherwise report a PAUSED voice's held position
        // so the UI playhead does not snap to zero on stop.
        for voice in &self.voices {
            if voice.is_active {
                return voice.play_head as u64;
            }
        }
        for voice in &self.voices {
            if voice.buffer.is_some() && voice.play_head > 0.0 {
                return voice.play_head as u64;
            }
        }
        0
    }
}

impl SamplerProcessor {
    pub fn apply_command_with_context(&mut self, command: &nullherz_traits::ProcessorCommand, context: Option<&nullherz_traits::ProcessContext>) {
        use nullherz_traits::{Command, PerformanceCommand};
        match *command {
            // Bus-delivered parameters (playback rate, quantize, slicer,
            // grid). The sampler had NO SetParam arm, so every parameter
            // sent over the command bus to a sampler was silently dropped.
            Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, .. }) if target_id == self.id => {
                self.set_parameter(param_id, value);
            }
            Command::Performance(PerformanceCommand::JumpToHotCue { node_idx, cue_idx }) if node_idx as u64 == self.id => {
                let offset = if let Some(ref metadata) = self.metadata {
                    metadata.hot_cues.get(cue_idx as usize).and_then(|&c| c)
                        .unwrap_or((cue_idx as f32 * 0.1 * self.source_frames() as f32) as u64)
                } else {
                    (cue_idx as f32 * 0.1 * self.source_frames() as f32) as u64
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

                // Applies to paused voices as well: pause deactivates a
                // voice but keeps it armed, and CUE on a paused deck must
                // re-seat the held position.
                for voice in self.voices.iter_mut() {
                    if voice.is_active || voice.buffer.is_some() {
                        voice.play_head = offset as f32;
                    }
                }
            }
            Command::Performance(PerformanceCommand::TriggerSlice { node_idx, slice_idx }) if node_idx as u64 == self.id => {
                self.trigger_slice(slice_idx, context);
            }
            Command::Performance(PerformanceCommand::JumpByBeats { node_idx, beats }) if node_idx as u64 == self.id => {
                let source_frames = self.source_frames();
                if let (Some(ctx), Some(meta)) = (context, &self.metadata)
                    && let Some(transport) = ctx.transport
                        && meta.bpm > 0.0 {
                            let samples_per_beat = (transport.sample_rate * 60.0 / meta.bpm) as f64;
                            let jump_samples = (beats as f64 * samples_per_beat) as f32;

                            // Paused voices jump too — pause holds a voice
                            // inactive with its position, and beat-jumping a
                            // paused deck must move the held position.
                            for voice in self.voices.iter_mut() {
                                if voice.is_active || voice.buffer.is_some() {
                                    // Law of Bit-Exact Reset: Jumps should be precise and not introduce DC offsets
                                    // Linear interpolation in voice handles the fractional position
                                    voice.play_head += jump_samples;

                                    // Clamp to buffer range
                                    if voice.play_head < 0.0 { voice.play_head = 0.0; }
                                    if voice.play_head >= source_frames as f32 {
                                        voice.play_head = (source_frames as f32 - 1.0).max(0.0);
                                    }
                                }
                            }
                        }
            }
            Command::Performance(PerformanceCommand::SetLoop { node_idx, enabled, start_samples, end_samples }) if node_idx as u64 == self.id => {
                for voice in self.voices.iter_mut() {
                    voice.loop_enabled = enabled;
                    voice.loop_start = start_samples;
                    voice.loop_end = end_samples;
                }
            }
            // TARGETED: PlayNode/StopNode are broadcast to every node by the
            // engine, so each sampler must check the address. Matching `..`
            // here meant playing ONE deck armed/triggered every sampler in
            // the graph — decks, preview, the sampler view — and any of them
            // auto-fired the moment a source later landed (pending_play).
            Command::Performance(PerformanceCommand::PlayNode { node_idx }) if node_idx as u64 == self.id => {
                if self.sample_buffer.is_empty() {
                    self.pending_play = true;
                } else {
                    let frames = self.source_frames();
                    // RESUME takes priority over a fresh trigger: a paused
                    // voice (deactivated by StopNode, position held) picks up
                    // where it left off. It must still belong to the CURRENT
                    // source and sit inside the buffer — AddSource clears
                    // voices, so a stale resume cannot replay an old track.
                    let resumable = self.voices.iter_mut().find(|v| {
                        !v.is_active
                            && v.play_head > 0.0
                            && (v.play_head as usize) < frames
                            && v.buffer.as_ref().is_some_and(|b| std::sync::Arc::ptr_eq(b, &self.sample_buffer))
                    });
                    if let Some(voice) = resumable {
                        voice.is_active = true;
                    } else {
                        let channels = self.source_channels();
                        if let Some(voice) = self.voices.iter_mut().find(|v| !v.is_active) {
                            let beat_pos = context.and_then(|c| c.transport).map(|t| t.beat_position).unwrap_or(0.0);
                            voice.trigger_at_ref(&self.sample_buffer, self.playback_rate, 1.0, 0.0, beat_pos);
                            Self::apply_layout(voice, frames, channels);
                        }
                    }
                }
            }
            Command::Performance(PerformanceCommand::StopNode { node_idx }) if node_idx as u64 == self.id => {
                // PAUSE, not wipe: a DJ's stop holds the position (CUE is the
                // way back to the start). Voices keep their play_head and
                // buffer; a following PlayNode resumes them in place.
                self.pending_play = false;
                for v in self.voices.iter_mut() {
                    v.is_active = false;
                }
            }
            Command::Performance(PerformanceCommand::SetSlipMode { node_idx, enabled }) if node_idx as u64 == self.id => {
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
