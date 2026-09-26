use std::any::Any;
use nullherz_traits::{AudioProcessor, SignalProcessor, ProcessContext, MidiResponder, SnapshotProvider};

pub struct ReactionDiffusionSidecar {
    pub speed: f32,
    pub feed_rate: f32,
}

impl ReactionDiffusionSidecar {
    pub fn new() -> Self { Self { speed: 1.0, feed_rate: 0.5 } }
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
