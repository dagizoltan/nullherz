use std::sync::Arc;
use nullherz_dna::SampleRegistry;
use nullherz_traits::{RenderingEngine, SampleMetadata};
use audio_dsp::TransientDetector;

/// Manages the registration and lifecycle of audio DNA (samples) captured by the engine.
/// This component acts as the non-RT side of the 'Transfusion' synthesis layer.
pub struct TransfusionManager {
    /// The global registry where captured samples are stored for use by other processors.
    pub sample_registry: Arc<SampleRegistry>,
    transient_detector: TransientDetector,
}

impl TransfusionManager {
    pub fn new(sample_registry: Arc<SampleRegistry>) -> Self {
        Self {
            sample_registry,
            transient_detector: TransientDetector::new(1024, 0.5),
        }
    }

    /// Polls the engine for new snapshots and registers them in the `SampleRegistry`.
    pub fn poll_snapshots(&mut self, engine: &dyn RenderingEngine) {
        let mut snapshots = Vec::new();
        engine.pull_all_snapshots(&mut snapshots);

        for (sample_id, snapshot) in snapshots {
            // Basic Transient Analysis: Check for onsets in the capture
            // We use the first 1024 samples for a quick look if enough data is present.
            let mut transients = Vec::new();
            if snapshot.len() >= 1024 {
                let re = &snapshot[0..1024];
                let im = vec![0.0; 1024]; // Assuming time-domain capture for analysis
                if self.transient_detector.is_transient(re, &im) {
                    transients.push(0);
                }
            }

            let metadata = SampleMetadata {
                bpm: 128.0, // Default for testing sync
                transients: Arc::new(transients),
                root_key: None,
                hot_cues: [None; 8],
                loop_points: None,
                beat_grid_offset: 0,
                peaks: Arc::new(Vec::new()),
                dna: nullherz_traits::SoundDNA::default(),
                midi_map: None,
            };

            self.sample_registry.register_with_metadata(sample_id, snapshot, metadata);
            eprintln!("Registered new transfusion source with metadata: ID={}", sample_id);

            // Also notify the topology manager to update the processor if it's currently active.
            // This is a bit of a hack for now, as we'd ideally want a more structured way to update sources.
            // We'll use AddSource for now, which is handled by Granular and Sampler.
            // We don't know the node_idx here easily, so we skip for now or broadcast.
        }
    }
}
