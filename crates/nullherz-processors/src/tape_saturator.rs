use nullherz_traits::{
    AudioProcessor, Command, ProcessContext, ProcessorCapability, ProcessorCommand,
    ProcessorFactory, ProcessorMetadata, ProcessorTypeId, SignalProcessor,
};
use audio_dsp::{AlignedBuffer, Filter, ZdfSvf};

pub struct TapeSaturatorProcessor {
    pub id: u64,
    pub drive: f32,
    pub high_cut_freq: f32,
    pub wow_flutter_depth: f32,
    pub wow_flutter_rate: f32,
    pub output_gain: f32,
    sample_rate: f32,
    head_gap_filter: ZdfSvf,
    delay_buffer: AlignedBuffer,
    write_ptr: usize,
    lfo_phase: f32,
}

impl TapeSaturatorProcessor {
    pub fn new(id: u64, sample_rate: f32) -> Self {
        let high_cut_freq = 14000.0;
        let mut filter = ZdfSvf::new(sample_rate);
        filter.set_params(high_cut_freq, 0.707);

        Self {
            id,
            drive: 1.5,
            high_cut_freq,
            wow_flutter_depth: 0.05,
            wow_flutter_rate: 0.5,
            output_gain: 1.0,
            sample_rate,
            head_gap_filter: filter,
            delay_buffer: AlignedBuffer::new(512),
            write_ptr: 0,
            lfo_phase: 0.0,
        }
    }
}

impl nullherz_traits::RtSafe for TapeSaturatorProcessor {}

impl SignalProcessor for TapeSaturatorProcessor {
    fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        if config.sample_rate > 0.0 {
            self.sample_rate = config.sample_rate;
            self.head_gap_filter = ZdfSvf::new(config.sample_rate);
            self.head_gap_filter.set_params(self.high_cut_freq, 0.707);
        }
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }

        let input = inputs[0];
        let output = &mut outputs[0];
        let len = input.len().min(output.len());
        let buf_len = self.delay_buffer.len();

        let lfo_inc = 2.0 * core::f32::consts::PI * self.wow_flutter_rate / self.sample_rate;
        let drive = self.drive;
        let out_gain = self.output_gain;
        let wf_depth = self.wow_flutter_depth * 10.0; // Up to 10 samples modulation

        for i in 0..len {
            let in_sample = input[i];
            let driven = in_sample * drive;

            // Soft-clip tape saturation using rational Padé approximant
            let saturated = (driven * 0.8).tanh();

            // Head gap high-frequency loss filtering
            let filtered = self.head_gap_filter.process_lp(saturated);

            // Write to ring buffer for wow/flutter modulation
            self.delay_buffer[self.write_ptr] = filtered;

            // LFO modulation for mechanical tape speed variation
            let mod_delay = 5.0 + (self.lfo_phase.sin() * wf_depth);
            self.lfo_phase += lfo_inc;
            if self.lfo_phase >= 2.0 * core::f32::consts::PI {
                self.lfo_phase -= 2.0 * core::f32::consts::PI;
            }

            // Read from modulated delay line using linear interpolation
            let read_pos = (self.write_ptr as f32 + buf_len as f32 - mod_delay) % buf_len as f32;
            let idx_floor = read_pos as usize % buf_len;
            let idx_next = (idx_floor + 1) % buf_len;
            let frac = read_pos - read_pos.floor();

            let wow_sample = self.delay_buffer[idx_floor] * (1.0 - frac) + self.delay_buffer[idx_next] * frac;

            self.write_ptr = (self.write_ptr + 1) % buf_len;

            let final_sample = wow_sample * out_gain;
            output[i] = if final_sample.is_finite() { final_sample } else { 0.0 };
        }
    }

    fn reset(&mut self) {
        self.head_gap_filter.reset();
        self.delay_buffer.fill(0.0);
        self.write_ptr = 0;
        self.lfo_phase = 0.0;
    }
}

impl nullherz_traits::MidiResponder for TapeSaturatorProcessor {
    fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&ProcessContext>) {}
}

impl nullherz_traits::SnapshotProvider for TapeSaturatorProcessor {}

impl AudioProcessor for TapeSaturatorProcessor {
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
            0 => self.drive = value.clamp(1.0, 10.0),
            1 => {
                self.high_cut_freq = value.clamp(2000.0, 20000.0);
                self.head_gap_filter.set_params(self.high_cut_freq, 0.707);
            }
            2 => self.wow_flutter_depth = value.clamp(0.0, 1.0),
            3 => self.wow_flutter_rate = value.clamp(0.1, 10.0),
            4 => self.output_gain = value.clamp(0.0, 4.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.drive,
            1 => self.high_cut_freq,
            2 => self.wow_flutter_depth,
            3 => self.wow_flutter_rate,
            4 => self.output_gain,
            _ => 0.0,
        }
    }

    fn metadata(&self) -> Option<ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 10.0,
            default: 1.0,
        }; 16];

        let names: &[&[u8]] = &[b"Drive", b"HighCut", b"WowDepth", b"WowRate", b"Gain"];
        let mins = [1.0, 2000.0, 0.0, 0.1, 0.0];
        let maxs = [10.0, 20000.0, 1.0, 10.0, 4.0];
        let defs = [1.5, 14000.0, 0.05, 0.5, 1.0];

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

pub struct TapeSaturatorFactory;

impl ProcessorFactory for TapeSaturatorFactory {
    fn type_id(&self) -> ProcessorTypeId {
        ProcessorTypeId::TAPE_SATURATOR
    }

    fn name(&self) -> &'static str {
        "TapeSaturator"
    }

    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(TapeSaturatorProcessor::new(node_idx as u64, sample_rate)))
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
