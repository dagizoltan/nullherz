use std::sync::Arc;
use nullherz_dna::SampleRegistry;
use nullherz_traits::RenderingEngine;

pub struct TransfusionManager {
    pub sample_registry: Arc<SampleRegistry>,
}

impl TransfusionManager {
    pub fn new(sample_registry: Arc<SampleRegistry>) -> Self {
        Self { sample_registry }
    }

    pub fn poll_snapshots(&mut self, engine: &mut dyn RenderingEngine) {
        let mut snapshots = Vec::new();
        engine.pull_all_snapshots(&mut snapshots);

        for (sample_id, snapshot) in snapshots {
            self.sample_registry.register(sample_id, snapshot);
            eprintln!("Registered new transfusion source: ID={}", sample_id);
        }
    }
}
