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

    /// Drop the registry's reference to a sample, returning it to the caller.
    ///
    /// Deliberately has NO default implementation. A default returning `None`
    /// would let an implementor silently never evict while callers believed
    /// residency was bounded — and unbounded residency is exactly the bug this
    /// exists to fix (a 500-track library scan held every decoded track in RAM
    /// forever, tens of gigabytes).
    ///
    /// **The returned value is the point.** The registry's `Arc` is dropped
    /// wherever the caller drops the return value, so an eviction driven from a
    /// background thread frees there. Discarding it inline is fine ONLY off the
    /// audio thread: if a sampler currently holds the last other reference,
    /// dropping the final `Arc` is a multi-megabyte `free()`, and on the
    /// SCHED_FIFO thread that is a dropout. Evicting a sample that is loaded on
    /// a deck therefore needs a garbage-return channel that does not exist yet
    /// — until it does, only evict samples no processor holds.
    ///
    /// Returns `None` if the id was not registered.
    #[must_use = "dropping the evicted sample here frees it on THIS thread; ensure that is not the audio thread"]
    fn remove(&self, id: u64) -> Option<RegisteredSample>;
}

pub trait CommandBundleConsumer: Send {
    fn pop(&mut self) -> Option<Vec<Command>>;
}

