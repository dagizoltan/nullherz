pub mod sidecar;
pub mod sampler;
pub mod factory;
pub mod registry;
pub mod gain;
pub mod biquad;
pub mod crossfader;
pub mod summing;
pub mod wavetable;
pub mod spectral;
pub mod dsp_kernel_processor;
pub mod modulation;
pub mod sequencer;
pub mod transfusion;
#[cfg(feature = "test-utils")]
pub mod test_kit;

pub use nullherz_traits::{MAX_CHANNELS, MAX_NODES};

pub use sidecar::SidecarProcessor;
pub use sampler::SamplerProcessor;
pub use gain::GainProcessor;
pub use biquad::{BiquadProcessor, SimdBiquadProcessor};
pub use crossfader::CrossfaderProcessor;
pub use summing::SummingProcessor;
pub use wavetable::WavetableProcessor;
pub use spectral::SpectralProcessor;
pub use modulation::ModulationProcessor;
pub use sequencer::SequencerProcessor;
pub use transfusion::*;
pub use registry::ProcessorRegistry;
