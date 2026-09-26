//! Standalone Neural Network Driven Visual Surface Sidecar
//! Executes zero-allocation neural network forward pass driven by live audio
//! and telemetry characteristics (spectrum, phase vectors, DNA latent space).

use std::any::Any;
use nullherz_traits::{
    AudioProcessor, SignalProcessor, ProcessContext, MidiResponder, SnapshotProvider,
};

/// 16-Neuron Spiking Neural Network Engine for Standalone Visual Sidecars
pub struct SpikingNeuronEngine {
    pub v: [f32; 16],
    pub u: [f32; 16],
    pub spikes: [bool; 16],
    pub motor_output: f32,
}

impl SpikingNeuronEngine {
    pub fn new() -> Self {
        Self {
            v: [-65.0; 16],
            u: [-13.0; 16],
            spikes: [false; 16],
            motor_output: 0.0,
        }
    }

    pub fn step(&mut self, audio_input: f32, neural_temp: f32) -> f32 {
        let a = 0.02f32;
        let b = 0.2f32;
        let c = -65.0f32;
        let d = 8.0f32;

        let mut sum_v = 0.0f32;
        for i in 0..16 {
            let current = audio_input * 15.0 * neural_temp + (i as f32 * 0.5).sin() * 2.0;
            let v = self.v[i];
            let u = self.u[i];

            let dv = 0.04 * v * v + 5.0 * v + 140.0 - u + current;
            let du = a * (b * v - u);

            let next_v = v + dv * 0.5;
            let next_u = u + du * 0.5;

            if next_v >= 30.0 {
                self.v[i] = c;
                self.u[i] = next_u + d;
                self.spikes[i] = true;
            } else {
                self.v[i] = next_v.clamp(-90.0, 30.0);
                self.u[i] = next_u;
                self.spikes[i] = false;
            }
            sum_v += (self.v[i] + 65.0) / 30.0;
        }
        self.motor_output = sum_v / 16.0;
        self.motor_output
    }
}

pub struct NeuralVisualsSidecar {
    pub name: String,
    pub neural_temperature: f32,
    pub feedback_coupling: f32,
    pub gain_sensitivity: f32,
    pub spiking_engine: SpikingNeuronEngine,
}

impl NeuralVisualsSidecar {
    pub fn new() -> Self {
        Self {
            name: "Neural Visual Surface Sidecar".to_string(),
            neural_temperature: 1.0,
            feedback_coupling: 0.5,
            gain_sensitivity: 1.0,
            spiking_engine: SpikingNeuronEngine::new(),
        }
    }

    /// Padé SIMD Rational Activation: tanh_approx(x)
    #[inline]
    fn pade_tanh(x: f32) -> f32 {
        let x2 = x * x;
        (x * (27.0 + x2) / (27.0 + 9.0 * x2)).clamp(-1.0, 1.0)
    }

    /// Execute forward pass neural synthesis transformation on an input audio sample pair
    pub fn synthesize_frame(&mut self, l: f32, r: f32, time_secs: f32) -> (f32, f32) {
        let input_energy = ((l * l + r * r) * 0.5).sqrt() * self.gain_sensitivity;
        let phase_vector = l - r;

        let spike_motor = self.spiking_engine.step(input_energy, self.neural_temperature);

        let h1 = Self::pade_tanh(input_energy * self.neural_temperature + spike_motor + (time_secs * 2.0).sin());
        let h2 = Self::pade_tanh(h1 * (1.0 + self.feedback_coupling) + phase_vector);

        (h1, h2)
    }
}

impl Default for NeuralVisualsSidecar {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalProcessor for NeuralVisualsSidecar {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext<'_>) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let in_l = inputs[0];
        let in_r = if inputs.len() > 1 { inputs[1] } else { in_l };

        let (out_left, out_right) = outputs.split_at_mut(1);
        let out_l = &mut out_left[0];

        let len = in_l.len().min(out_l.len());
        for i in 0..len {
            let l = in_l[i];
            let r = in_r.get(i).copied().unwrap_or(l);
            let (synth_l, synth_r) = self.synthesize_frame(l, r, i as f32 / 44100.0);
            out_l[i] = l + synth_l * 0.1;

            if !out_right.is_empty() && i < out_right[0].len() {
                out_right[0][i] = r + synth_r * 0.1;
            }
        }
    }
}

impl MidiResponder for NeuralVisualsSidecar {}

impl SnapshotProvider for NeuralVisualsSidecar {}

impl AudioProcessor for NeuralVisualsSidecar {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        let safe_val = if value.is_finite() { value } else { 1.0 };
        match param_id {
            0 => self.neural_temperature = safe_val.clamp(0.0, 4.0),
            1 => self.feedback_coupling = safe_val.clamp(0.0, 1.0),
            2 => self.gain_sensitivity = safe_val.clamp(0.0, 2.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.neural_temperature,
            1 => self.feedback_coupling,
            2 => self.gain_sensitivity,
            _ => 0.0,
        }
    }

    fn as_any(&self) -> &(dyn Any + 'static) {
        self
    }
    fn as_any_mut(&mut self) -> &mut (dyn Any + 'static) {
        self
    }
}

fn main() {
    println!("Starting Standalone Neural Visual Surface Sidecar...");
    let mut sidecar = NeuralVisualsSidecar::new();

    // Smoke test neural synthesis loop
    let (l, r) = sidecar.synthesize_frame(0.8, -0.4, 0.5);
    println!("Neural visual sidecar online. Test output frame: ({:.4}, {:.4})", l, r);
}
