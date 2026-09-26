use std::any::Any;
use nullherz_traits::{AudioProcessor, SignalProcessor, ProcessContext, MidiResponder, SnapshotProvider};

pub struct ShaderParticleSwarmSidecar {
    pub particle_density: f32,
    pub swarm_velocity: f32,
}

impl ShaderParticleSwarmSidecar {
    pub fn new() -> Self { Self { particle_density: 1.0, swarm_velocity: 1.0 } }
}

impl Default for ShaderParticleSwarmSidecar {
    fn default() -> Self { Self::new() }
}

impl SignalProcessor for ShaderParticleSwarmSidecar {
    fn process(&mut self, _inputs: &[&[f32]], _outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext<'_>) {}
}

impl MidiResponder for ShaderParticleSwarmSidecar {}
impl SnapshotProvider for ShaderParticleSwarmSidecar {}

impl AudioProcessor for ShaderParticleSwarmSidecar {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp: u32) {
        let safe = if value.is_finite() { value } else { 1.0 };
        match param_id {
            0 => self.particle_density = safe.clamp(0.1, 2.0),
            1 => self.swarm_velocity = safe.clamp(0.1, 3.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.particle_density,
            1 => self.swarm_velocity,
            _ => 0.0,
        }
    }

    fn as_any(&self) -> &(dyn Any + 'static) { self }
    fn as_any_mut(&mut self) -> &mut (dyn Any + 'static) { self }
}

fn main() {
    println!("Shader Particle Swarm Visual Sidecar Online.");
}
