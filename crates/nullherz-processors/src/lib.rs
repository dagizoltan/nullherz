pub mod standard;
pub mod sidecar;
pub mod complex;
pub mod sampler;
pub mod factory;

pub const MAX_CHANNELS: usize = 16;
pub const MAX_NODES: usize = 64;

pub use sidecar::SidecarProcessor;
pub use sampler::SamplerProcessor;
pub use standard::{GainProcessor, BiquadProcessor, SimdBiquadProcessor, CrossfaderProcessor, SummingProcessor};
pub use complex::{WavetableProcessor, SpectralProcessor, ModulationProcessor, SequencerProcessor};
