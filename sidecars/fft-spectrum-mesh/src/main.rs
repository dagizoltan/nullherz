use std::any::Any;
use nullherz_traits::{AudioProcessor, SignalProcessor, ProcessContext, MidiResponder, SnapshotProvider};

pub struct FftSpectrumMeshSidecar {
    pub resolution: f32,
    pub tilt: f32,
}

impl FftSpectrumMeshSidecar {
    pub fn new() -> Self { Self { resolution: 1.0, tilt: 0.5 } }
}

impl Default for FftSpectrumMeshSidecar {
    fn default() -> Self { Self::new() }
}

impl SignalProcessor for FftSpectrumMeshSidecar {
    fn process(&mut self, _inputs: &[&[f32]], _outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext<'_>) {}
}

impl MidiResponder for FftSpectrumMeshSidecar {}
impl SnapshotProvider for FftSpectrumMeshSidecar {}

impl AudioProcessor for FftSpectrumMeshSidecar {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp: u32) {
        let safe = if value.is_finite() { value } else { 1.0 };
        match param_id {
            0 => self.resolution = safe.clamp(0.1, 2.0),
            1 => self.tilt = safe.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.resolution,
            1 => self.tilt,
            _ => 0.0,
        }
    }

    fn as_any(&self) -> &(dyn Any + 'static) { self }
    fn as_any_mut(&mut self) -> &mut (dyn Any + 'static) { self }
}

fn main() {
    println!("FFT Spectrum Mesh Visual Sidecar Online.");
}
