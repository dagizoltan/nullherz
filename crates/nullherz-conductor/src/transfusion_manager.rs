use std::sync::Arc;
use nullherz_dna::SampleRegistry;
use nullherz_traits::RenderingEngine;

/// Manages the registration and lifecycle of audio DNA (samples) captured by the engine.
/// This component acts as the non-RT side of the 'Transfusion' synthesis layer.
pub struct TransfusionManager {
    /// The global registry where captured samples are stored for use by other processors.
    pub sample_registry: Arc<SampleRegistry>,
}

impl TransfusionManager {
    pub fn new(sample_registry: Arc<SampleRegistry>) -> Self {
        Self { sample_registry }
    }

    /// Polls the engine for new snapshots and registers them in the `SampleRegistry`.
    pub fn poll_snapshots(&mut self, engine: &mut dyn RenderingEngine) {
        let mut snapshots = Vec::new();
        engine.pull_all_snapshots(&mut snapshots);

        for (sample_id, snapshot) in snapshots {
            self.sample_registry.register(sample_id, snapshot);
            eprintln!("Registered new transfusion source: ID={}", sample_id);
        }
    }
}
