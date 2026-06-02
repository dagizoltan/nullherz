pub mod backends;
pub mod processors;
pub mod engine;
pub mod telemetry;
pub mod error;

pub use engine::AudioEngine;
pub use processors::{AudioProcessor, ProcessorGraph, SidecarProcessor, ProcessorNode, GraphTopology, NodeRouting, CrossfadeState};
pub use backends::{AudioBackend, AlsaBackend, PipewireBackend, JackBackend, ThreadedBackend};
pub use telemetry::Telemetry;

pub const MAX_CHANNELS: usize = 16;
