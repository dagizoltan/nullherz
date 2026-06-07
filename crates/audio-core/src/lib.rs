pub mod backends;
pub mod processors;
pub mod engine;
pub mod telemetry;
pub mod error;

pub use engine::AudioEngine;
pub use processors::{AudioProcessor, ProcessorGraph, SidecarProcessor, ProcessorNode, GraphTopology, NodeRouting, CrossfadeState};
pub use backends::{AudioBackend, AlsaBackend, PipewireBackend, JackBackend, ThreadedBackend};
pub use telemetry::Telemetry;

#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    pub sample_rate: f32,
    pub block_size: usize,
}

pub const MAX_CHANNELS: usize = 16;
