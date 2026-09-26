use std::any::Any;
use nullherz_traits::{AudioProcessor, SignalProcessor, ProcessContext, MidiResponder, SnapshotProvider};

pub struct PhaseGoniometerSidecar {
    pub gain: f32,
    pub width: f32,
}

impl PhaseGoniometerSidecar {
    pub fn new() -> Self { Self { gain: 1.0, width: 1.0 } }
}

impl Default for PhaseGoniometerSidecar {
    fn default() -> Self { Self::new() }
}

impl SignalProcessor for PhaseGoniometerSidecar {
    fn process(&mut self, _inputs: &[&[f32]], _outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext<'_>) {}
}

impl MidiResponder for PhaseGoniometerSidecar {}
impl SnapshotProvider for PhaseGoniometerSidecar {}

impl AudioProcessor for PhaseGoniometerSidecar {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp: u32) {
        let safe = if value.is_finite() { value } else { 1.0 };
        match param_id {
            0 => self.gain = safe.clamp(0.0, 2.0),
            1 => self.width = safe.clamp(0.0, 2.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.gain,
            1 => self.width,
            _ => 0.0,
        }
    }

    fn as_any(&self) -> &(dyn Any + 'static) { self }
    fn as_any_mut(&mut self) -> &mut (dyn Any + 'static) { self }
}

fn main() {
    println!("Phase Goniometer 2D Visual Sidecar Online.");
}
