pub mod graph;

pub use graph::{ProcessorGraph, ProcessorNode, GraphTopology, NodeRouting, CrossfadeState, TaskPool, GraphCompiler};
pub use nullherz_processors::{
    SidecarProcessor, SamplerProcessor, GainProcessor, BiquadProcessor, SimdBiquadProcessor,
    CrossfaderProcessor, SummingProcessor, WavetableProcessor, SpectralProcessor, ModulationProcessor, SequencerProcessor
};

pub use nullherz_traits::{
    AudioProcessor, ProcessContext as GenericProcessContext, AudioConfig, Transport,
    TopologyMutation, ProcessorCommand, MidiEvent, GarbageProducer
};

/// Engine-specific process context.
pub type ProcessContext<'a> = GenericProcessContext<'a>;
