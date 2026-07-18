use sidecar_sdk::{AudioProcessor, ProcessContext};

pub struct BitcrusherSidecar {
    bits: f32,
    downsample: u32,
    counter: u32,
    last_sample: f32,
}

impl Default for BitcrusherSidecar {
    fn default() -> Self {
        Self::new()
    }
}

impl BitcrusherSidecar {
    pub fn new() -> Self {
        Self {
            bits: 16.0,
            downsample: 1,
            counter: 0,
            last_sample: 0.0,
        }
    }
}

impl nullherz_traits::SignalProcessor for BitcrusherSidecar {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        let input = inputs[0];
        let output = &mut outputs[0];

        let step = (2.0f32.powf(self.bits)).recip();

        for i in 0..input.len() {
            if self.counter.is_multiple_of(self.downsample) {
                let s = input[i];
                self.last_sample = (s / step).round() * step;
            }
            output[i] = self.last_sample;
            self.counter += 1;
        }
    }
}

impl nullherz_traits::MidiResponder for BitcrusherSidecar {}
impl nullherz_traits::SnapshotProvider for BitcrusherSidecar {}

impl AudioProcessor for BitcrusherSidecar {
    fn apply_command(&mut self, cmd: &nullherz_traits::Command) {
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { param_id, value, .. }) = cmd {
            match param_id {
                0 => self.bits = value.clamp(1.0, 24.0),
                1 => self.downsample = (*value as u32).max(1),
                _ => {}
            }
        }
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

sidecar_macros::sidecar_builder!();

fn main() {
    SidecarApp::build_and_run("Bitcrusher", BitcrusherSidecar::new());
}
