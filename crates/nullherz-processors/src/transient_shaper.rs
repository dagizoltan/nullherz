use nullherz_traits::{
    AudioProcessor, Command, ProcessContext, ProcessorCapability, ProcessorCommand,
    ProcessorFactory, ProcessorMetadata, ProcessorTypeId, SignalProcessor,
};
use audio_dsp::{AlignedBuffer, EnvelopeFollower, DspKernel};

pub struct TransientShaperProcessor {
    pub id: u64,
    fast_env: EnvelopeFollower,
    slow_env: EnvelopeFollower,
    pub attack_gain: f32,
    pub sustain_gain: f32,
    pub output_gain: f32,
    pub attack_speed_ms: f32,
    pub sustain_speed_ms: f32,
    env_fast_buf: AlignedBuffer,
    env_slow_buf: AlignedBuffer,
}

impl TransientShaperProcessor {
    pub fn new(id: u64, sample_rate: f32) -> Self {
        let attack_speed_ms = 1.0;
        let sustain_speed_ms = 100.0;
        Self {
            id,
            fast_env: EnvelopeFollower::new(sample_rate, attack_speed_ms, 20.0),
            slow_env: EnvelopeFollower::new(sample_rate, sustain_speed_ms, 200.0),
            attack_gain: 1.0,
            sustain_gain: 1.0,
            output_gain: 1.0,
            attack_speed_ms,
            sustain_speed_ms,
            env_fast_buf: AlignedBuffer::new(nullherz_traits::MAX_BLOCK_SIZE),
            env_slow_buf: AlignedBuffer::new(nullherz_traits::MAX_BLOCK_SIZE),
        }
    }
}

impl nullherz_traits::RtSafe for TransientShaperProcessor {}

impl SignalProcessor for TransientShaperProcessor {
    fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        if config.sample_rate > 0.0 {
            self.fast_env = EnvelopeFollower::new(config.sample_rate, self.attack_speed_ms, 20.0);
            self.slow_env = EnvelopeFollower::new(config.sample_rate, self.sustain_speed_ms, 200.0);
        }
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }

        let input = inputs[0];
        let output = &mut outputs[0];
        let len = input.len().min(output.len());

        if self.env_fast_buf.len() < len || self.env_slow_buf.len() < len {
            return;
        }

        self.fast_env.process_block(input, &mut self.env_fast_buf[..len]);
        self.slow_env.process_block(input, &mut self.env_slow_buf[..len]);

        let attack_mult = self.attack_gain;
        let sustain_mult = self.sustain_gain;
        let out_gain = self.output_gain;

        for i in 0..len {
            let fast = self.env_fast_buf[i];
            let slow = self.env_slow_buf[i];
            let transient = (fast - slow).max(0.0);
            let sustain = slow;

            let mod_gain = 1.0 + (attack_mult - 1.0) * transient + (sustain_mult - 1.0) * sustain;
            let sample = input[i] * mod_gain * out_gain;
            output[i] = if sample.is_finite() { sample } else { 0.0 };
        }
    }

    fn reset(&mut self) {
        self.fast_env.reset();
        self.slow_env.reset();
    }
}

impl nullherz_traits::MidiResponder for TransientShaperProcessor {
    fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&ProcessContext>) {}
}

impl nullherz_traits::SnapshotProvider for TransientShaperProcessor {}

impl AudioProcessor for TransientShaperProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, .. }) = command
            && *target_id == self.id {
            self.set_parameter(*param_id, *value, 0);
        }
    }

    fn set_parameter(&mut self, param_id: u32, mut value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() { value = 1.0; }
        match param_id {
            0 => self.attack_gain = value.clamp(0.0, 4.0),
            1 => self.sustain_gain = value.clamp(0.0, 4.0),
            2 => self.output_gain = value.clamp(0.0, 4.0),
            3 => {
                self.attack_speed_ms = value.clamp(0.1, 10.0);
                self.fast_env.set_times(self.attack_speed_ms, 20.0);
            }
            4 => {
                self.sustain_speed_ms = value.clamp(10.0, 500.0);
                self.slow_env.set_times(self.sustain_speed_ms, 200.0);
            }
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.attack_gain,
            1 => self.sustain_gain,
            2 => self.output_gain,
            3 => self.attack_speed_ms,
            4 => self.sustain_speed_ms,
            _ => 0.0,
        }
    }

    fn metadata(&self) -> Option<ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 4.0,
            default: 1.0,
        }; 16];

        let names: &[&[u8]] = &[b"Attack", b"Sustain", b"Gain", b"AttackSpeed", b"SustainSpeed"];
        let mins = [0.0, 0.0, 0.0, 0.1, 10.0];
        let maxs = [4.0, 4.0, 4.0, 10.0, 500.0];
        let defs = [1.0, 1.0, 1.0, 1.0, 100.0];

        for (i, &name) in names.iter().enumerate() {
            parameters[i].id = i as u32;
            parameters[i].name[..name.len()].copy_from_slice(name);
            parameters[i].min = mins[i];
            parameters[i].max = maxs[i];
            parameters[i].default = defs[i];
        }

        Some(ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 5,
            parameters,
        })
    }
}

pub struct TransientShaperFactory;

impl ProcessorFactory for TransientShaperFactory {
    fn type_id(&self) -> ProcessorTypeId {
        ProcessorTypeId::TRANSIENT_SHAPER
    }

    fn name(&self) -> &'static str {
        "TransientShaper"
    }

    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(TransientShaperProcessor::new(node_idx as u64, sample_rate)))
    }

    fn capabilities(&self) -> ProcessorCapability {
        ProcessorCapability {
            supports_parallel: true,
            is_instrument: false,
            has_midi_input: false,
            has_audio_input: true,
            has_audio_output: true,
        }
    }
}
