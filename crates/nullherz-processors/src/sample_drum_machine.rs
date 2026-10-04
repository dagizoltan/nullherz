use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand, SampleBuffer, TopologyMutation, Command,
};

const NUM_PADS: usize = 16;

#[derive(Clone, Debug)]
pub struct SamplePadConfig {
    pub name: [u8; 32],
    pub sample_buffer: Option<SampleBuffer>,
    pub sample_id: Option<u64>,
    pub start_crop: f32, // 0.0 .. 1.0
    pub end_crop: f32,   // 0.0 .. 1.0
    pub pitch_semitones: f32, // -24.0 .. +24.0
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    pub transient_boost: f32,
    pub choke_group: u8,
    pub output_channel: u8, // 0..15
}

impl Default for SamplePadConfig {
    fn default() -> Self {
        let mut name = [0u8; 32];
        let default_name = b"Sample Pad";
        name[..default_name.len()].copy_from_slice(default_name);

        Self {
            name,
            sample_buffer: None,
            sample_id: None,
            start_crop: 0.0,
            end_crop: 1.0,
            pitch_semitones: 0.0,
            attack_ms: 1.0,
            decay_ms: 200.0,
            sustain: 0.0,
            release_ms: 50.0,
            transient_boost: 1.0,
            choke_group: 0,
            output_channel: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EnvelopeStage {
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Debug)]
struct PadVoice {
    play_head: f64,
    start_frame: f64,
    end_frame: f64,
    rate: f32,
    velocity: f32,
    is_active: bool,
    stage: EnvelopeStage,
    env_level: f32,
    env_counter: u32,
}

impl Default for PadVoice {
    fn default() -> Self {
        Self {
            play_head: 0.0,
            start_frame: 0.0,
            end_frame: 0.0,
            rate: 1.0,
            velocity: 0.0,
            is_active: false,
            stage: EnvelopeStage::Off,
            env_level: 0.0,
            env_counter: 0,
        }
    }
}

/// Sampler-Based Multi-Output Drum Machine Processor
pub struct SampleDrumMachineProcessor {
    pub node_id: u64,
    pub pads: [SamplePadConfig; NUM_PADS],
    voices: [PadVoice; NUM_PADS],
    sample_rate: f32,
}

impl SampleDrumMachineProcessor {
    pub fn new(node_id: u64, sample_rate: f32) -> Self {
        let pads = std::array::from_fn(|i| {
            let mut pad = SamplePadConfig::default();
            pad.output_channel = i as u8;
            pad.pad_index_label(i as u8);
            pad
        });

        audio_dsp::resample::prewarm();

        Self {
            node_id,
            pads,
            voices: std::array::from_fn(|_| PadVoice::default()),
            sample_rate: if sample_rate > 0.0 { sample_rate } else { nullherz_traits::DEFAULT_SAMPLE_RATE },
        }
    }

    pub fn trigger_pad(&mut self, pad_idx: usize, velocity: f32) {
        if pad_idx >= NUM_PADS { return; }

        let choke_group = self.pads[pad_idx].choke_group;
        if choke_group > 0 {
            for (i, pad) in self.pads.iter().enumerate() {
                if i != pad_idx && pad.choke_group == choke_group {
                    self.voices[i].is_active = false;
                    self.voices[i].stage = EnvelopeStage::Off;
                }
            }
        }

        let pad = &self.pads[pad_idx];
        let Some(ref buf) = pad.sample_buffer else { return; };
        let total_frames = buf.len();
        if total_frames == 0 { return; }

        let start_f = (pad.start_crop.clamp(0.0, 1.0) * total_frames as f32) as f64;
        let end_f = (pad.end_crop.clamp(0.0, 1.0) * total_frames as f32).max(start_f as f32 + 1.0) as f64;
        let end_f = end_f.min(total_frames as f64);

        let rate = 2.0f32.powf(pad.pitch_semitones / 12.0);

        let voice = &mut self.voices[pad_idx];
        voice.play_head = start_f;
        voice.start_frame = start_f;
        voice.end_frame = end_f;
        voice.rate = rate;
        voice.velocity = velocity.clamp(0.0, 1.0);
        voice.is_active = true;
        voice.stage = EnvelopeStage::Attack;
        voice.env_level = 0.0;
        voice.env_counter = 0;
    }

    pub fn choke_pad(&mut self, pad_idx: usize) {
        if pad_idx < NUM_PADS {
            self.voices[pad_idx].stage = EnvelopeStage::Release;
        }
    }
}

impl SamplePadConfig {
    fn pad_index_label(&mut self, idx: u8) {
        let name_str = format!("Pad {:02}", idx + 1);
        let bytes = name_str.as_bytes();
        let len = bytes.len().min(32);
        self.name[..len].copy_from_slice(&bytes[..len]);
    }
}

impl SignalProcessor for SampleDrumMachineProcessor {
    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext) {
        if outputs.is_empty() { return; }
        let num_samples = outputs[0].len();

        for out in outputs.iter_mut() {
            out[..num_samples].fill(0.0);
        }

        let sr = self.sample_rate;
        let sinc_table = audio_dsp::resample::table();

        for p_idx in 0..NUM_PADS {
            let voice = &mut self.voices[p_idx];
            if !voice.is_active { continue; }

            let pad = &self.pads[p_idx];
            let Some(ref buf) = pad.sample_buffer else {
                voice.is_active = false;
                continue;
            };

            let out_channel = (pad.output_channel as usize) % outputs.len().max(1);
            let attack_samples = ((pad.attack_ms * 0.001) * sr).max(1.0) as u32;
            let decay_samples = ((pad.decay_ms * 0.001) * sr).max(1.0) as u32;
            let release_samples = ((pad.release_ms * 0.001) * sr).max(1.0) as u32;
            let sustain_lvl = pad.sustain.clamp(0.0, 1.0);
            let boost = pad.transient_boost.max(0.0);

            let buffer_slice = buf.as_slice();
            let buf_len = buffer_slice.len();

            for i in 0..num_samples {
                if !voice.is_active { break; }

                if voice.play_head >= voice.end_frame || voice.play_head >= buf_len as f64 {
                    voice.is_active = false;
                    voice.stage = EnvelopeStage::Off;
                    break;
                }

                // High-quality 16-tap windowed sinc interpolation for pitch-shifting
                let raw_sample = sinc_table.sample(buffer_slice, voice.play_head, voice.rate.abs());

                // ADSR envelope calculation
                match voice.stage {
                    EnvelopeStage::Attack => {
                        voice.env_counter += 1;
                        voice.env_level = (voice.env_counter as f32 / attack_samples as f32).min(1.0);
                        if voice.env_counter >= attack_samples {
                            voice.stage = EnvelopeStage::Decay;
                            voice.env_counter = 0;
                        }
                    }
                    EnvelopeStage::Decay => {
                        voice.env_counter += 1;
                        let progress = (voice.env_counter as f32 / decay_samples as f32).min(1.0);
                        voice.env_level = 1.0 - progress * (1.0 - sustain_lvl);
                        if voice.env_counter >= decay_samples {
                            voice.stage = EnvelopeStage::Sustain;
                            voice.env_counter = 0;
                        }
                    }
                    EnvelopeStage::Sustain => {
                        voice.env_level = sustain_lvl;
                        if sustain_lvl <= 0.001 {
                            voice.stage = EnvelopeStage::Release;
                            voice.env_counter = 0;
                        }
                    }
                    EnvelopeStage::Release => {
                        voice.env_counter += 1;
                        let progress = (voice.env_counter as f32 / release_samples as f32).min(1.0);
                        voice.env_level = sustain_lvl * (1.0 - progress);
                        if voice.env_counter >= release_samples || voice.env_level <= 0.0001 {
                            voice.is_active = false;
                            voice.stage = EnvelopeStage::Off;
                        }
                    }
                    EnvelopeStage::Off => {
                        voice.is_active = false;
                    }
                }

                let out_val = raw_sample * voice.env_level * voice.velocity * boost;
                if out_val.is_finite() && out_channel < outputs.len() {
                    outputs[out_channel][i] += out_val;
                }

                voice.play_head += voice.rate as f64;
            }
        }
    }

    fn reset(&mut self) {
        for v in self.voices.iter_mut() {
            v.is_active = false;
            v.stage = EnvelopeStage::Off;
            v.play_head = 0.0;
        }
    }

    fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        if config.sample_rate > 0.0 {
            self.sample_rate = config.sample_rate;
        }
    }
}

impl MidiResponder for SampleDrumMachineProcessor {
    fn apply_midi(&mut self, event: ipc_layer::MidiEvent, _context: Option<&ProcessContext>) {
        let status = event.status & 0xF0;
        if status == 0x90 && event.data2 > 0 {
            let note = event.data1;
            let pad_idx = if (36..=51).contains(&note) {
                (note - 36) as usize
            } else {
                (note % 16) as usize
            };
            let velocity = event.data2 as f32 / 127.0;
            self.trigger_pad(pad_idx, velocity);
        } else if status == 0x80 || (status == 0x90 && event.data2 == 0) {
            let note = event.data1;
            let pad_idx = if (36..=51).contains(&note) {
                (note - 36) as usize
            } else {
                (note % 16) as usize
            };
            self.choke_pad(pad_idx);
        }
    }
}

impl SnapshotProvider for SampleDrumMachineProcessor {}

impl AudioProcessor for SampleDrumMachineProcessor {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        let pad_idx = (param_id / 16) as usize;
        let p_offset = param_id % 16;
        if pad_idx >= NUM_PADS { return; }

        let val = if value.is_finite() { value } else { 0.0 };
        let pad = &mut self.pads[pad_idx];

        match p_offset {
            0 => pad.start_crop = val.clamp(0.0, 1.0),
            1 => pad.end_crop = val.clamp(0.0, 1.0),
            2 => pad.pitch_semitones = val.clamp(-24.0, 24.0),
            3 => pad.attack_ms = val.clamp(0.1, 10000.0),
            4 => pad.decay_ms = val.clamp(0.1, 10000.0),
            5 => pad.sustain = val.clamp(0.0, 1.0),
            6 => pad.release_ms = val.clamp(0.1, 10000.0),
            7 => pad.transient_boost = val.clamp(0.0, 10.0),
            8 => pad.choke_group = (val as u8).clamp(0, 16),
            9 => pad.output_channel = (val as u8).clamp(0, 15),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        let pad_idx = (param_id / 16) as usize;
        let p_offset = param_id % 16;
        if pad_idx >= NUM_PADS { return 0.0; }

        let pad = &self.pads[pad_idx];
        match p_offset {
            0 => pad.start_crop,
            1 => pad.end_crop,
            2 => pad.pitch_semitones,
            3 => pad.attack_ms,
            4 => pad.decay_ms,
            5 => pad.sustain,
            6 => pad.release_ms,
            7 => pad.transient_boost,
            8 => pad.choke_group as f32,
            9 => pad.output_channel as f32,
            _ => 0.0,
        }
    }

    fn apply_topology_mutation(&mut self, mutation: TopologyMutation) {
        if let TopologyMutation::AddSource { node_idx, buffer, sample_id, .. } = mutation {
            let pad_idx = (node_idx as usize) % NUM_PADS;
            self.pads[pad_idx].sample_buffer = Some(buffer);
            self.pads[pad_idx].sample_id = Some(sample_id);
        }
    }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        match command {
            Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, ramp_duration_samples, .. }) if *target_id == self.node_id => {
                self.set_parameter(*param_id, *value, *ramp_duration_samples);
            }
            Command::Performance(nullherz_traits::PerformanceCommand::SetSequencerStep { node_idx, track, value, .. }) if *node_idx as u64 == self.node_id => {
                if *value > 0.0 {
                    self.trigger_pad((*track as usize) % NUM_PADS, *value);
                }
            }
            Command::Performance(nullherz_traits::PerformanceCommand::EvolvePattern { node_idx, track_idx, mutation_strength }) if *node_idx as u64 == self.node_id => {
                let p = (*track_idx as usize) % NUM_PADS;
                let pad = &mut self.pads[p];
                pad.pitch_semitones = (pad.pitch_semitones + (mutation_strength * 2.0 - 1.0)).clamp(-24.0, 24.0);
                pad.decay_ms = (pad.decay_ms * (1.0 + mutation_strength * 0.2)).clamp(10.0, 2000.0);
            }
            _ => {}
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
