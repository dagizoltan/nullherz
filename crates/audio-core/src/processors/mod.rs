pub mod standard;
pub mod sidecar;
pub mod complex;
pub mod graph;

pub use graph::{ProcessorGraph, ProcessorNode, GraphTopology, NodeRouting, CrossfadeState, TaskPool};
pub use sidecar::SidecarProcessor;
pub use standard::{GainProcessor, BiquadProcessor, SimdBiquadProcessor, CrossfaderProcessor, SummingProcessor};
pub use complex::{WavetableProcessor, SpectralProcessor, ModulationProcessor};

pub struct ProcessContext<'a> {
    pub pool: Option<&'a mut TaskPool>,
}

pub trait AudioProcessor: Send {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext);
    fn setup(&mut self, _config: crate::AudioConfig) {}
    fn apply_command(&mut self, _command: &control_plane::Command) {}
    fn apply_topology_command(&mut self, _command: &control_plane::TopologyCommand) {}
    fn collect_telemetry(&self, _node_times: &mut [u64; 64], _peak_levels: &mut [f32; 64]) {}
    fn set_garbage_producer(&mut self, _producer: ipc_layer::Producer<Box<dyn AudioProcessor>>) {}
}
