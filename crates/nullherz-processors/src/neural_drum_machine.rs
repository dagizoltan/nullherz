use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand, SoundDNA, Command,
};

const NUM_PADS: usize = 16;

#[inline(always)]
fn pade_tanh(x: f32) -> f32 {
    let x2 = x * x;
    let num = x * (1.0 + 0.12317192 * x2);
    let den = 1.0 + 0.4565311 * x2 + 0.01524316 * x2 * x2;
    (num / den).clamp(-1.0, 1.0)
}

/// 64-Neuron Spiking Cortical Resonator modeling organic drumhead shell resonance
#[derive(Clone, Debug)]
pub struct SpikingCorticalResonator {
    pub v: [f32; 64],             // Membrane potentials (mV)
    pub u: [f32; 64],             // Recovery variables
    pub s: [f32; 64],             // Synaptic neurotransmitter conductance
    pub spikes: [bool; 64],       // Current frame spike triggers
    pub stdp_trace: [f32; 64],    // STDP activity traces
    pub weights: [[f32; 64]; 64], // 64x64 Recurrent synaptic weight matrix
    pub axonal_grid: [f32; 64],   // 2D 8x8 spatial axonal grid
}

impl SpikingCorticalResonator {
    pub fn new() -> Self {
        let mut weights = [[0.0f32; 64]; 64];
        for i in 0..64 {
            let row_i = i / 8;
            let col_i = i % 8;
            for j in 0..64 {
                if i != j {
                    let row_j = j / 8;
                    let col_j = j % 8;
                    let dist_sq = ((row_i as f32 - row_j as f32).powi(2) + (col_i as f32 - col_j as f32).powi(2)).max(0.5);
                    let spatial_decay = (-dist_sq * 0.25).exp();
                    let periodic = (i as f32 * 0.4 + j as f32 * 0.9).sin();
                    weights[i][j] = periodic * spatial_decay * 0.35;
                }
            }
        }

        Self {
            v: [-65.0; 64],
            u: [-13.0; 64],
            s: [0.0; 64],
            spikes: [false; 64],
            stdp_trace: [0.0; 64],
            weights,
            axonal_grid: [0.0; 64],
        }
    }

    pub fn trigger_stroke(&mut self, pad_idx: usize, velocity: f32) {
        let base_neuron = (pad_idx * 4) % 64;
        for i in 0..4 {
            let idx = (base_neuron + i) % 64;
            self.v[idx] += velocity * 25.0;
            self.s[idx] = (self.s[idx] + velocity).min(3.0);
            self.axonal_grid[idx] += velocity * 1.5;
        }
    }

    pub fn step(&mut self, dt: f32) -> f32 {
        let a = 0.02f32;
        let b = 0.2f32;
        let c = -65.0f32;
        let d = 8.0f32;

        let dt_ms = (dt * 1000.0).clamp(0.1, 5.0);

        let mut currents = [0.0f32; 64];
        for from in 0..64 {
            let transmitter = self.s[from];
            if transmitter > 0.01 {
                for to in 0..64 {
                    currents[to] += transmitter * self.weights[from][to] * 12.0;
                }
            }
        }

        // 2D Spatial Axonal Grid Laplacian Diffusion
        let mut new_grid = self.axonal_grid;
        for r in 0..8 {
            for c_idx in 0..8 {
                let idx = r * 8 + c_idx;
                let neighbor_sum =
                    self.axonal_grid[((r + 1) % 8) * 8 + c_idx] +
                    self.axonal_grid[((r + 7) % 8) * 8 + c_idx] +
                    self.axonal_grid[r * 8 + (c_idx + 1) % 8] +
                    self.axonal_grid[r * 8 + (c_idx + 7) % 8];
                let laplacian = neighbor_sum - 4.0 * self.axonal_grid[idx];
                new_grid[idx] += laplacian * 0.15;
                currents[idx] += new_grid[idx] * 2.0;
            }
        }
        self.axonal_grid = new_grid;

        let mut resonance_sum = 0.0f32;
        for i in 0..64 {
            let v = self.v[i];
            let u = self.u[i];
            let i_ext = currents[i];

            let dv = (0.04 * v * v + 5.0 * v + 140.0 - u + i_ext) * dt_ms;
            let du = (a * (b * v - u)) * dt_ms;

            let next_v = v + dv;
            let next_u = u + du;

            if next_v >= 30.0 {
                self.v[i] = c;
                self.u[i] = next_u + d;
                self.s[i] = (self.s[i] + 1.0).min(3.0);
                self.spikes[i] = true;
                self.stdp_trace[i] = (self.stdp_trace[i] + 1.0).min(4.0);
                self.axonal_grid[i] += 1.2;
                resonance_sum += 1.0;
            } else {
                self.v[i] = next_v.clamp(-90.0, 30.0);
                self.u[i] = next_u;
                self.s[i] *= 0.90;
                self.spikes[i] = false;
                self.stdp_trace[i] *= 0.94;
                self.axonal_grid[i] *= 0.92;
                resonance_sum += (self.v[i] + 65.0) / 95.0;
            }
        }

        // STDP Synaptic Weight Plasticity Adaptation
        for i in 0..64 {
            if self.spikes[i] {
                for j in 0..64 {
                    if i != j {
                        let delta_w = 0.0012 * (self.stdp_trace[j] - 0.25);
                        self.weights[j][i] = (self.weights[j][i] + delta_w).clamp(-1.2, 1.2);
                    }
                }
            }
        }

        pade_tanh(resonance_sum / 64.0)
    }
}

/// Latent Space Timbre Decoder: Decodes 16-D coordinates into drum synthesis parameters
#[derive(Clone, Debug)]
pub struct LatentTimbreDecoder {
    pub latent_coords: [f32; 16],
    pub sound_dna: SoundDNA,
}

impl LatentTimbreDecoder {
    pub fn new(latent_coords: [f32; 16]) -> Self {
        Self {
            latent_coords,
            sound_dna: SoundDNA::default(),
        }
    }

    /// Decode 16-D latent coordinates to synthesis parameters
    pub fn decode_params(&self) -> (f32, f32, f32, f32, f32, f32) {
        let start_freq = 50.0 + (self.latent_coords[0] * 0.5 + 0.5).clamp(0.0, 1.0) * 350.0;
        let end_freq = 30.0 + (self.latent_coords[1] * 0.5 + 0.5).clamp(0.0, 1.0) * 120.0;
        let decay_ms = 20.0 + (self.latent_coords[2] * 0.5 + 0.5).clamp(0.0, 1.0) * 780.0;
        let noise_blend = (self.latent_coords[3] * 0.5 + 0.5).clamp(0.0, 1.0);
        let cutoff_hz = 500.0 + (self.latent_coords[4] * 0.5 + 0.5).clamp(0.0, 1.0) * 11500.0;
        let drive = 0.5 + (self.latent_coords[5] * 0.5 + 0.5).clamp(0.0, 1.0) * 4.5;

        (start_freq, end_freq, decay_ms, noise_blend, cutoff_hz, drive)
    }
}

/// Neural TCN / SSM Shell Shaper for non-linear drum shell saturation and analog transformer warmth
#[derive(Clone, Debug)]
pub struct NeuralShellShaper {
    pub state_a: f32,
    pub state_b: f32,
    pub drive: f32,
}

impl NeuralShellShaper {
    pub fn new() -> Self {
        Self {
            state_a: 0.0,
            state_b: 0.0,
            drive: 1.5,
        }
    }

    pub fn process_sample(&mut self, input: f32) -> f32 {
        let x = input * self.drive;
        let u = x + self.state_a * 0.15;
        let saturated = pade_tanh(u);

        self.state_a = self.state_b + (x - saturated) * 0.3;
        self.state_b = saturated * 0.2;

        saturated
    }
}

#[derive(Clone, Debug)]
struct NeuralVoiceState {
    phase: f32,
    amp_env: f32,
    velocity: f32,
    is_active: bool,
}

impl Default for NeuralVoiceState {
    fn default() -> Self {
        Self {
            phase: 0.0,
            amp_env: 0.0,
            velocity: 0.0,
            is_active: false,
        }
    }
}

/// Neural Network Driven Multi-Output Drum Machine Processor
pub struct NeuralDrumMachineProcessor {
    pub node_id: u64,
    pub decoders: [LatentTimbreDecoder; NUM_PADS],
    pub resonator: SpikingCorticalResonator,
    pub shell_shapers: [NeuralShellShaper; NUM_PADS],
    pub output_channels: [u8; NUM_PADS],
    voices: [NeuralVoiceState; NUM_PADS],
    sample_rate: f32,
}

impl NeuralDrumMachineProcessor {
    pub fn new(node_id: u64, sample_rate: f32) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { nullherz_traits::DEFAULT_SAMPLE_RATE };

        let decoders = std::array::from_fn(|i| {
            let mut coords = [0.0f32; 16];
            coords[i % 16] = 0.8;
            coords[(i + 1) % 16] = -0.4;
            LatentTimbreDecoder::new(coords)
        });

        let shell_shapers = std::array::from_fn(|_| NeuralShellShaper::new());
        let output_channels = std::array::from_fn(|i| i as u8);

        Self {
            node_id,
            decoders,
            resonator: SpikingCorticalResonator::new(),
            shell_shapers,
            output_channels,
            voices: std::array::from_fn(|_| NeuralVoiceState::default()),
            sample_rate: sr,
        }
    }

    pub fn trigger_pad(&mut self, pad_idx: usize, velocity: f32) {
        if pad_idx >= NUM_PADS { return; }

        self.resonator.trigger_stroke(pad_idx, velocity);

        let voice = &mut self.voices[pad_idx];
        voice.phase = 0.0;
        voice.amp_env = velocity.clamp(0.0, 1.0);
        voice.velocity = velocity.clamp(0.0, 1.0);
        voice.is_active = true;
    }
}

impl SignalProcessor for NeuralDrumMachineProcessor {
    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext) {
        if outputs.is_empty() { return; }
        let num_samples = outputs[0].len();

        for out in outputs.iter_mut() {
            out[..num_samples].fill(0.0);
        }

        let sr = self.sample_rate;
        let dt = 1.0 / sr;

        for i in 0..num_samples {
            let shell_resonance = self.resonator.step(dt);

            for pad_idx in 0..NUM_PADS {
                let voice = &mut self.voices[pad_idx];
                if !voice.is_active { continue; }

                let (start_freq, end_freq, decay_ms, _noise_blend, _cutoff_hz, drive) = self.decoders[pad_idx].decode_params();

                let env_coeff = (-1.0 / (sr * (decay_ms * 0.001).max(0.001))).exp();
                voice.amp_env *= env_coeff;

                if voice.amp_env < 0.0001 {
                    voice.is_active = false;
                    continue;
                }

                let current_freq = end_freq + (start_freq - end_freq) * voice.amp_env;
                let phase_inc = (current_freq * std::f32::consts::TAU) / sr;
                voice.phase = (voice.phase + phase_inc) % std::f32::consts::TAU;

                let synth_tone = voice.phase.sin() * voice.amp_env;
                let resonated = synth_tone + shell_resonance * 0.15 * voice.amp_env;

                let shaper = &mut self.shell_shapers[pad_idx];
                shaper.drive = drive;
                let shaped = shaper.process_sample(resonated) * voice.velocity;

                let out_ch = (self.output_channels[pad_idx] as usize) % outputs.len().max(1);
                if out_ch < outputs.len() {
                    outputs[out_ch][i] += shaped;
                }
            }
        }
    }

    fn reset(&mut self) {
        for v in self.voices.iter_mut() {
            v.is_active = false;
            v.amp_env = 0.0;
        }
    }

    fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        if config.sample_rate > 0.0 {
            self.sample_rate = config.sample_rate;
        }
    }
}

impl MidiResponder for NeuralDrumMachineProcessor {
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

impl SnapshotProvider for NeuralDrumMachineProcessor {}

impl AudioProcessor for NeuralDrumMachineProcessor {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        let pad_idx = (param_id / 16) as usize;
        let p_offset = param_id % 16;
        if pad_idx >= NUM_PADS { return; }

        let val = if value.is_finite() { value } else { 0.0 };

        if p_offset < 16 {
            self.decoders[pad_idx].latent_coords[p_offset as usize] = val.clamp(-1.0, 1.0);
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        let pad_idx = (param_id / 16) as usize;
        let p_offset = param_id % 16;
        if pad_idx >= NUM_PADS { return 0.0; }

        if p_offset < 16 {
            self.decoders[pad_idx].latent_coords[p_offset as usize]
        } else {
            0.0
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
