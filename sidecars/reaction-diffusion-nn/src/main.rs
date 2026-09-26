use std::any::Any;
use nullherz_traits::{AudioProcessor, SignalProcessor, ProcessContext, MidiResponder, SnapshotProvider};

/// Spiking Neural Network Engine for Reaction-Diffusion Sidecar
pub struct SpikingNeuronEngine {
    pub v: [f32; 16],
    pub u: [f32; 16],
    pub spikes: [bool; 16],
}

impl SpikingNeuronEngine {
    pub fn new() -> Self {
        Self {
            v: [-65.0; 16],
            u: [-13.0; 16],
            spikes: [false; 16],
        }
    }

    pub fn step(&mut self, audio_input: f32, speed: f32) {
        let a = 0.02f32;
        let b = 0.2f32;
        let c = -65.0f32;
        let d = 8.0f32;

        for i in 0..16 {
            let current = audio_input * 15.0 * speed + (i as f32 * 0.4).sin() * 2.0;
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
        }
    }
}

pub struct ReactionDiffusionSidecar {
    pub speed: f32,
    pub feed_rate: f32,
    pub spiking_engine: SpikingNeuronEngine,
}

impl ReactionDiffusionSidecar {
    pub fn new() -> Self { Self { speed: 1.0, feed_rate: 0.5, spiking_engine: SpikingNeuronEngine::new() } }
}

impl Default for ReactionDiffusionSidecar {
    fn default() -> Self { Self::new() }
}

impl SignalProcessor for ReactionDiffusionSidecar {
    fn process(&mut self, _inputs: &[&[f32]], _outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext<'_>) {}
}

impl MidiResponder for ReactionDiffusionSidecar {}
impl SnapshotProvider for ReactionDiffusionSidecar {}

impl AudioProcessor for ReactionDiffusionSidecar {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp: u32) {
        let safe = if value.is_finite() { value } else { 1.0 };
        match param_id {
            0 => self.speed = safe.clamp(0.1, 3.0),
            1 => self.feed_rate = safe.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.speed,
            1 => self.feed_rate,
            _ => 0.0,
        }
    }

    fn as_any(&self) -> &(dyn Any + 'static) { self }
    fn as_any_mut(&mut self) -> &mut (dyn Any + 'static) { self }
}

fn main() {
    println!("Reaction Diffusion NN Visual Sidecar Online.");
}
