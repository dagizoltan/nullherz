pub mod gain;
pub mod biquad;
pub mod sidecar;
pub mod sampler;
pub mod wavetable;
pub mod spectral;
pub mod summing;
pub mod crossfader;
pub mod modulation;
pub mod sequencer;
pub mod registry;
pub mod factory;

pub use registry::ProcessorRegistry;
pub use nullherz_traits::ProcessorFactory;
pub use sidecar::SidecarProcessor;

use nullherz_traits::{AudioProcessor, MidiHandler, CommandHandler, TopologyHandler, TelemetryProvider, ProcessContext, ProcessorCommand, Command};
use audio_dsp::DspKernel;

pub struct DspKernelProcessor<K: DspKernel> {
    pub kernel: K,
    pub id: u64,
}

impl<K: DspKernel> DspKernelProcessor<K> {
    pub fn new(id: u64, kernel: K) -> Self {
        Self { kernel, id }
    }
}

impl<K: DspKernel> MidiHandler for DspKernelProcessor<K> {}
impl<K: DspKernel> TopologyHandler for DspKernelProcessor<K> {}
impl<K: DspKernel> TelemetryProvider for DspKernelProcessor<K> {}

impl<K: DspKernel> CommandHandler for DspKernelProcessor<K> {
    fn apply_command(&mut self, command: &ProcessorCommand) {
        match *command {
            Command::SetParam { target_id, param_id, value, ramp_duration_samples }
                if target_id == self.id =>
            {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
            _ => {}
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32, ramp_duration_samples: u32) {
        self.kernel.set_parameter(param_id, value, ramp_duration_samples);
    }
}

impl<K: DspKernel + 'static> AudioProcessor for DspKernelProcessor<K> {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        self.kernel.process(inputs, outputs);
    }

    fn reset(&mut self) {
        self.kernel.reset();
    }
}

pub use nullherz_traits::{MAX_CHANNELS, MAX_NODES};
