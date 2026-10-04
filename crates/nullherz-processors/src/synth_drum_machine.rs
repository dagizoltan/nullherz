use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand, Command,
};
use audio_dsp::ZdfSvf;

const NUM_PADS: usize = 16;

#[inline(always)]
fn pade_tanh(x: f32) -> f32 {
    let x2 = x * x;
    let num = x * (1.0 + 0.12317192 * x2);
    let den = 1.0 + 0.4565311 * x2 + 0.01524316 * x2 * x2;
    (num / den).clamp(-1.0, 1.0)
}

/// Simple xorshift LFG pseudo-random generator for zero-allocation noise on RT thread
#[derive(Clone, Debug)]
struct FastNoise {
    state: u32,
}

impl FastNoise {
    fn new(seed: u32) -> Self {
        Self { state: if seed == 0 { 0x12345678 } else { seed } }
    }

    #[inline(always)]
    fn next_f32(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Analog Kick Synth Generator
#[derive(Clone, Debug)]
pub struct KickSynthProcessor {
    pub start_pitch_hz: f32,
    pub end_pitch_hz: f32,
    pub pitch_decay_ms: f32,
    pub amp_decay_ms: f32,
    pub drive: f32,
    pub click_level: f32,
    phase: f32,
    current_freq: f32,
    amp_env: f32,
    pitch_env: f32,
    click_env: f32,
    is_active: bool,
}

impl KickSynthProcessor {
    pub fn new() -> Self {
        Self {
            start_pitch_hz: 150.0,
            end_pitch_hz: 45.0,
            pitch_decay_ms: 35.0,
            amp_decay_ms: 350.0,
            drive: 1.5,
            click_level: 0.8,
            phase: 0.0,
            current_freq: 45.0,
            amp_env: 0.0,
            pitch_env: 0.0,
            click_env: 0.0,
            is_active: false,
        }
    }

    pub fn trigger(&mut self, velocity: f32) {
        self.phase = 0.0;
        self.current_freq = self.start_pitch_hz;
        self.amp_env = velocity.clamp(0.0, 1.0);
        self.pitch_env = 1.0;
        self.click_env = 1.0;
        self.is_active = true;
    }

    pub fn process_sample(&mut self, sample_rate: f32) -> f32 {
        if !self.is_active { return 0.0; }

        let pitch_coeff = (-1.0 / (sample_rate * (self.pitch_decay_ms * 0.001).max(0.001))).exp();
        let amp_coeff = (-1.0 / (sample_rate * (self.amp_decay_ms * 0.001).max(0.001))).exp();
        let click_coeff = (-1.0 / (sample_rate * 0.002)).exp(); // 2ms click

        self.pitch_env *= pitch_coeff;
        self.amp_env *= amp_coeff;
        self.click_env *= click_coeff;

        if self.amp_env < 0.0001 {
            self.is_active = false;
            return 0.0;
        }

        self.current_freq = self.end_pitch_hz + (self.start_pitch_hz - self.end_pitch_hz) * self.pitch_env;
        let phase_inc = (self.current_freq * std::f32::consts::TAU) / sample_rate;
        self.phase = (self.phase + phase_inc) % std::f32::consts::TAU;

        let body_osc = self.phase.sin();
        let click = self.click_env * self.click_level;

        let raw = (body_osc + click) * self.amp_env * self.drive;
        pade_tanh(raw)
    }
}

/// Analog Snare Synth Generator
#[derive(Clone, Debug)]
pub struct SnareSynthProcessor {
    pub body_start_hz: f32,
    pub body_end_hz: f32,
    pub body_decay_ms: f32,
    pub noise_decay_ms: f32,
    pub noise_blend: f32,
    pub filter_cutoff_hz: f32,
    body_phase: f32,
    body_env: f32,
    noise_env: f32,
    filter: ZdfSvf,
    noise_gen: FastNoise,
    is_active: bool,
}

impl SnareSynthProcessor {
    pub fn new(sample_rate: f32) -> Self {
        let mut filter = ZdfSvf::new(sample_rate);
        filter.set_params(2500.0, 1.2);

        Self {
            body_start_hz: 180.0,
            body_end_hz: 90.0,
            body_decay_ms: 120.0,
            noise_decay_ms: 220.0,
            noise_blend: 0.6,
            filter_cutoff_hz: 2500.0,
            body_phase: 0.0,
            body_env: 0.0,
            noise_env: 0.0,
            filter,
            noise_gen: FastNoise::new(0x98765432),
            is_active: false,
        }
    }

    pub fn trigger(&mut self, velocity: f32) {
        self.body_phase = 0.0;
        let vel = velocity.clamp(0.0, 1.0);
        self.body_env = vel;
        self.noise_env = vel;
        self.is_active = true;
    }

    pub fn process_sample(&mut self, sample_rate: f32) -> f32 {
        if !self.is_active { return 0.0; }

        let body_coeff = (-1.0 / (sample_rate * (self.body_decay_ms * 0.001).max(0.001))).exp();
        let noise_coeff = (-1.0 / (sample_rate * (self.noise_decay_ms * 0.001).max(0.001))).exp();

        self.body_env *= body_coeff;
        self.noise_env *= noise_coeff;

        if self.body_env < 0.0001 && self.noise_env < 0.0001 {
            self.is_active = false;
            return 0.0;
        }

        let body_freq = self.body_end_hz + (self.body_start_hz - self.body_end_hz) * self.body_env;
        let phase_inc = (body_freq * std::f32::consts::TAU) / sample_rate;
        self.body_phase = (self.body_phase + phase_inc) % std::f32::consts::TAU;

        let body_tone = self.body_phase.sin() * self.body_env;

        let raw_noise = self.noise_gen.next_f32();
        self.filter.set_params(self.filter_cutoff_hz, 1.2);
        let filtered_noise = self.filter.process_hp(raw_noise) * self.noise_env;

        let mix = body_tone * (1.0 - self.noise_blend) + filtered_noise * self.noise_blend;
        pade_tanh(mix * 1.5)
    }
}

/// Analog Hi-Hat Synth Generator
#[derive(Clone, Debug)]
pub struct HiHatSynthProcessor {
    pub decay_ms: f32,
    pub hp_cutoff_hz: f32,
    phases: [f32; 6],
    freqs: [f32; 6],
    vca_env: f32,
    filter: ZdfSvf,
    is_active: bool,
}

impl HiHatSynthProcessor {
    pub fn new(sample_rate: f32) -> Self {
        let mut filter = ZdfSvf::new(sample_rate);
        filter.set_params(7000.0, 2.0);

        Self {
            decay_ms: 60.0,
            hp_cutoff_hz: 7000.0,
            phases: [0.0; 6],
            freqs: [205.0, 300.0, 365.0, 520.0, 800.0, 870.0],
            vca_env: 0.0,
            filter,
            is_active: false,
        }
    }

    pub fn trigger(&mut self, velocity: f32, decay_override_ms: Option<f32>) {
        if let Some(d) = decay_override_ms {
            self.decay_ms = d;
        }
        self.vca_env = velocity.clamp(0.0, 1.0);
        self.is_active = true;
    }

    pub fn choke(&mut self) {
        self.vca_env = 0.0;
        self.is_active = false;
    }

    pub fn process_sample(&mut self, sample_rate: f32) -> f32 {
        if !self.is_active { return 0.0; }

        let env_coeff = (-1.0 / (sample_rate * (self.decay_ms * 0.001).max(0.001))).exp();
        self.vca_env *= env_coeff;

        if self.vca_env < 0.0001 {
            self.is_active = false;
            return 0.0;
        }

        // 6 inharmonic square wave ring modulators
        let mut square_sum = 0.0f32;
        for i in 0..6 {
            let inc = (self.freqs[i] * std::f32::consts::TAU) / sample_rate;
            self.phases[i] = (self.phases[i] + inc) % std::f32::consts::TAU;
            let sq = if self.phases[i] < std::f32::consts::PI { 1.0 } else { -1.0 };
            square_sum = if i == 0 { sq } else { square_sum * sq }; // Ring modulation
        }

        self.filter.set_params(self.hp_cutoff_hz, 2.0);
        let metallic_noise = self.filter.process_hp(square_sum);

        metallic_noise * self.vca_env
    }
}

/// Tom / Percussion Synth Generator
#[derive(Clone, Debug)]
pub struct TomPercSynthProcessor {
    pub start_freq_hz: f32,
    pub end_freq_hz: f32,
    pub decay_ms: f32,
    pub noise_click: f32,
    phase: f32,
    amp_env: f32,
    noise_gen: FastNoise,
    is_active: bool,
}

impl TomPercSynthProcessor {
    pub fn new() -> Self {
        Self {
            start_freq_hz: 220.0,
            end_freq_hz: 90.0,
            decay_ms: 250.0,
            noise_click: 0.3,
            phase: 0.0,
            amp_env: 0.0,
            noise_gen: FastNoise::new(0x456789ab),
            is_active: false,
        }
    }

    pub fn trigger(&mut self, velocity: f32) {
        self.phase = 0.0;
        self.amp_env = velocity.clamp(0.0, 1.0);
        self.is_active = true;
    }

    pub fn process_sample(&mut self, sample_rate: f32) -> f32 {
        if !self.is_active { return 0.0; }

        let env_coeff = (-1.0 / (sample_rate * (self.decay_ms * 0.001).max(0.001))).exp();
        self.amp_env *= env_coeff;

        if self.amp_env < 0.0001 {
            self.is_active = false;
            return 0.0;
        }

        let current_freq = self.end_freq_hz + (self.start_freq_hz - self.end_freq_hz) * self.amp_env;
        let phase_inc = (current_freq * std::f32::consts::TAU) / sample_rate;
        self.phase = (self.phase + phase_inc) % std::f32::consts::TAU;

        let sine_body = self.phase.sin();
        let click = if self.amp_env > 0.8 { self.noise_gen.next_f32() * self.noise_click } else { 0.0 };

        let mix = (sine_body + click) * self.amp_env;
        pade_tanh(mix * 1.3)
    }
}

pub enum SynthVoiceEngine {
    Kick(KickSynthProcessor),
    Snare(SnareSynthProcessor),
    HiHat(HiHatSynthProcessor),
    TomPerc(TomPercSynthProcessor),
}

/// Synthesizer-Based Multi-Output Drum Machine Processor
pub struct SynthDrumMachineProcessor {
    pub node_id: u64,
    pub voices: Vec<SynthVoiceEngine>,
    pub choke_groups: [u8; NUM_PADS],
    pub output_channels: [u8; NUM_PADS],
    sample_rate: f32,
}

impl SynthDrumMachineProcessor {
    pub fn new(node_id: u64, sample_rate: f32) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { nullherz_traits::DEFAULT_SAMPLE_RATE };

        let mut voices = Vec::with_capacity(NUM_PADS);
        for i in 0..NUM_PADS {
            let voice = match i {
                0 => SynthVoiceEngine::Kick(KickSynthProcessor::new()),
                1 => SynthVoiceEngine::Snare(SnareSynthProcessor::new(sr)),
                2 => SynthVoiceEngine::HiHat(HiHatSynthProcessor::new(sr)),
                3 => {
                    let mut hh = HiHatSynthProcessor::new(sr);
                    hh.decay_ms = 220.0; // Open hihat
                    SynthVoiceEngine::HiHat(hh)
                }
                _ => SynthVoiceEngine::TomPerc(TomPercSynthProcessor::new()),
            };
            voices.push(voice);
        }

        let mut choke_groups = [0u8; NUM_PADS];
        choke_groups[2] = 1; // Closed Hat
        choke_groups[3] = 1; // Open Hat

        let output_channels = std::array::from_fn(|i| i as u8);

        Self {
            node_id,
            voices,
            choke_groups,
            output_channels,
            sample_rate: sr,
        }
    }

    pub fn trigger_pad(&mut self, pad_idx: usize, velocity: f32) {
        if pad_idx >= NUM_PADS { return; }

        let choke_group = self.choke_groups[pad_idx];
        if choke_group > 0 {
            for i in 0..NUM_PADS {
                if i != pad_idx && self.choke_groups[i] == choke_group {
                    if let SynthVoiceEngine::HiHat(ref mut hh) = self.voices[i] {
                        hh.choke();
                    }
                }
            }
        }

        match &mut self.voices[pad_idx] {
            SynthVoiceEngine::Kick(k) => k.trigger(velocity),
            SynthVoiceEngine::Snare(s) => s.trigger(velocity),
            SynthVoiceEngine::HiHat(hh) => hh.trigger(velocity, None),
            SynthVoiceEngine::TomPerc(t) => t.trigger(velocity),
        }
    }
}

impl SignalProcessor for SynthDrumMachineProcessor {
    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext) {
        if outputs.is_empty() { return; }
        let num_samples = outputs[0].len();

        for out in outputs.iter_mut() {
            out[..num_samples].fill(0.0);
        }

        let sr = self.sample_rate;

        for pad_idx in 0..NUM_PADS.min(self.voices.len()) {
            let out_channel = (self.output_channels[pad_idx] as usize) % outputs.len().max(1);

            for i in 0..num_samples {
                let sample = match &mut self.voices[pad_idx] {
                    SynthVoiceEngine::Kick(k) => k.process_sample(sr),
                    SynthVoiceEngine::Snare(s) => s.process_sample(sr),
                    SynthVoiceEngine::HiHat(hh) => hh.process_sample(sr),
                    SynthVoiceEngine::TomPerc(t) => t.process_sample(sr),
                };

                if sample != 0.0 && out_channel < outputs.len() {
                    outputs[out_channel][i] += sample;
                }
            }
        }
    }

    fn reset(&mut self) {
        for voice in self.voices.iter_mut() {
            match voice {
                SynthVoiceEngine::Kick(k) => k.is_active = false,
                SynthVoiceEngine::Snare(s) => s.is_active = false,
                SynthVoiceEngine::HiHat(hh) => hh.choke(),
                SynthVoiceEngine::TomPerc(t) => t.is_active = false,
            }
        }
    }

    fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        if config.sample_rate > 0.0 {
            self.sample_rate = config.sample_rate;
        }
    }
}

impl MidiResponder for SynthDrumMachineProcessor {
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
        }
    }
}

impl SnapshotProvider for SynthDrumMachineProcessor {}

impl AudioProcessor for SynthDrumMachineProcessor {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        let pad_idx = (param_id / 16) as usize;
        let p_offset = param_id % 16;
        if pad_idx >= NUM_PADS || pad_idx >= self.voices.len() { return; }

        let val = if value.is_finite() { value } else { 0.0 };

        match p_offset {
            0 => {
                match &mut self.voices[pad_idx] {
                    SynthVoiceEngine::Kick(k) => { k.start_pitch_hz = 50.0 + val * 300.0; k.end_pitch_hz = 30.0 + val * 100.0; }
                    SynthVoiceEngine::Snare(s) => { s.body_start_hz = 100.0 + val * 300.0; s.body_end_hz = 50.0 + val * 150.0; }
                    SynthVoiceEngine::HiHat(hh) => { hh.hp_cutoff_hz = 3000.0 + val * 10000.0; }
                    SynthVoiceEngine::TomPerc(t) => { t.start_freq_hz = 100.0 + val * 400.0; t.end_freq_hz = 50.0 + val * 200.0; }
                }
            }
            1 => {
                let d_ms = (val * 2000.0).clamp(10.0, 5000.0);
                match &mut self.voices[pad_idx] {
                    SynthVoiceEngine::Kick(k) => k.amp_decay_ms = d_ms,
                    SynthVoiceEngine::Snare(s) => { s.body_decay_ms = d_ms; s.noise_decay_ms = d_ms * 1.5; }
                    SynthVoiceEngine::HiHat(hh) => hh.decay_ms = d_ms,
                    SynthVoiceEngine::TomPerc(t) => t.decay_ms = d_ms,
                }
            }
            2 => {
                match &mut self.voices[pad_idx] {
                    SynthVoiceEngine::Kick(k) => k.pitch_decay_ms = (val * 200.0).clamp(1.0, 500.0),
                    SynthVoiceEngine::Snare(s) => s.filter_cutoff_hz = 500.0 + val * 8000.0,
                    SynthVoiceEngine::HiHat(_) => {},
                    SynthVoiceEngine::TomPerc(t) => t.noise_click = val.clamp(0.0, 1.0),
                }
            }
            3 => {
                if let SynthVoiceEngine::Snare(s) = &mut self.voices[pad_idx] {
                    s.noise_blend = val.clamp(0.0, 1.0);
                }
            }
            4 => {
                if let SynthVoiceEngine::Kick(k) = &mut self.voices[pad_idx] {
                    k.drive = 0.5 + val * 4.0;
                }
            }
            6 => self.choke_groups[pad_idx] = (val as u8).clamp(0, 16),
            8 => self.output_channels[pad_idx] = (val as u8).clamp(0, 15),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        let pad_idx = (param_id / 16) as usize;
        let p_offset = param_id % 16;
        if pad_idx >= NUM_PADS || pad_idx >= self.voices.len() { return 0.0; }

        match p_offset {
            0 => match &self.voices[pad_idx] {
                SynthVoiceEngine::Kick(k) => ((k.start_pitch_hz - 50.0) / 300.0).clamp(0.0, 1.0),
                SynthVoiceEngine::Snare(s) => ((s.body_start_hz - 100.0) / 300.0).clamp(0.0, 1.0),
                SynthVoiceEngine::HiHat(hh) => ((hh.hp_cutoff_hz - 3000.0) / 10000.0).clamp(0.0, 1.0),
                SynthVoiceEngine::TomPerc(t) => ((t.start_freq_hz - 100.0) / 400.0).clamp(0.0, 1.0),
            },
            1 => match &self.voices[pad_idx] {
                SynthVoiceEngine::Kick(k) => (k.amp_decay_ms / 2000.0).clamp(0.0, 1.0),
                SynthVoiceEngine::Snare(s) => (s.body_decay_ms / 2000.0).clamp(0.0, 1.0),
                SynthVoiceEngine::HiHat(hh) => (hh.decay_ms / 2000.0).clamp(0.0, 1.0),
                SynthVoiceEngine::TomPerc(t) => (t.decay_ms / 2000.0).clamp(0.0, 1.0),
            },
            2 => match &self.voices[pad_idx] {
                SynthVoiceEngine::Kick(k) => (k.pitch_decay_ms / 200.0).clamp(0.0, 1.0),
                SynthVoiceEngine::Snare(s) => ((s.filter_cutoff_hz - 500.0) / 8000.0).clamp(0.0, 1.0),
                SynthVoiceEngine::HiHat(_) => 0.5,
                SynthVoiceEngine::TomPerc(t) => t.noise_click.clamp(0.0, 1.0),
            },
            3 => match &self.voices[pad_idx] {
                SynthVoiceEngine::Snare(s) => s.noise_blend,
                _ => 0.5,
            },
            4 => match &self.voices[pad_idx] {
                SynthVoiceEngine::Kick(k) => ((k.drive - 0.5) / 4.0).clamp(0.0, 1.0),
                _ => 0.5,
            },
            6 => self.choke_groups[pad_idx] as f32,
            8 => self.output_channels[pad_idx] as f32,
            _ => 0.5,
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
            _ => {}
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
