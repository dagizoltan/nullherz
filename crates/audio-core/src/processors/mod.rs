pub mod standard;
pub mod sidecar;
pub mod complex;
pub mod graph;

pub use graph::{ProcessorGraph, ProcessorNode, GraphTopology, NodeRouting, CrossfadeState, TaskPool};
pub use sidecar::SidecarProcessor;
pub use standard::{GainProcessor, BiquadProcessor, SimdBiquadProcessor, CrossfaderProcessor, SummingProcessor};
pub use complex::{WavetableProcessor, SpectralProcessor, ModulationProcessor};

/// Shared execution context passed to processors during the audio block cycle.
pub struct ProcessContext<'a> {
    /// Reference to the engine's worker task pool for parallel processing.
    pub pool: Option<&'a mut TaskPool>,
    /// Global transport information (BPM, position, play state).
    pub transport: Option<&'a crate::Transport>,
    /// Current sample offset within the physical audio block (used for sample-accurate automation).
    pub sub_block_offset: usize,
    /// Flag indicating if this is the final sub-block for the current engine cycle.
    pub is_last_sub_block: bool,
}

/// The core trait for all audio processing nodes in the nullherz engine.
pub trait AudioProcessor: Send {
    /// Executes audio processing for the given buffers.
    /// MUST be real-time safe: no allocations, no locks, no blocking syscalls.
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext);

    /// Called when audio configuration (sample rate, block size) changes.
    fn setup(&mut self, _config: crate::AudioConfig) {}

    /// Applies high-level control plane commands (parameters, play/stop).
    fn apply_command(&mut self, _command: &control_plane::Command) {}

    /// Applies structural graph mutations to the processor (routing, swapping).
    fn apply_topology_command(&mut self, _command: &control_plane::TopologyCommand) {}

    /// Gathers performance and signal telemetry from the processor.
    fn collect_telemetry(&self, _node_times: &mut [u64; 64], _peak_levels: &mut [f32; 64]) {}

    /// Configures the garbage producer used for real-time safe deallocation.
    fn set_garbage_producer(&mut self, _producer: ipc_layer::Producer<Box<dyn AudioProcessor>>) {}
}
