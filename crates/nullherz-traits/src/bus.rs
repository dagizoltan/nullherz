use std::sync::Arc;
use crate::*;

pub trait CommandProducer: Send + Sync + dyn_clone::DynClone {
    fn push_command(&self, command: TimestampedCommand) -> Result<(), Command>;
}

dyn_clone::clone_trait_object!(CommandProducer);

pub trait CommandConsumer: Send {
    fn pop_command(&mut self) -> Option<TimestampedCommand>;
}

pub trait TelemetryProducer: Send {
    fn push_telemetry(&mut self, telemetry: crate::telemetry::Telemetry) -> Result<(), crate::telemetry::Telemetry>;
}

pub trait MidiConsumer: Send {
    fn pop(&mut self) -> Option<MidiEvent>;
}

pub trait TopologyMutationConsumer: Send {
    fn pop(&mut self) -> Option<TopologyMutation>;
}

#[derive(Clone)]
pub struct RegisteredSample {
    pub buffer: Arc<Vec<f32>>,
    pub metadata: Arc<SampleMetadata>,
}

pub trait SampleRegistry: Send + Sync {
    fn get(&self, id: u64) -> Option<RegisteredSample>;
    fn register(&self, id: u64, buffer: Arc<Vec<f32>>);
    fn register_with_metadata(&self, id: u64, buffer: Arc<Vec<f32>>, metadata: Arc<SampleMetadata>);
    fn drain_garbage(&self);
    fn list_ids(&self) -> Vec<u64>;
}

pub trait CommandBundleConsumer: Send {
    fn pop(&mut self) -> Option<Vec<Command>>;
}

