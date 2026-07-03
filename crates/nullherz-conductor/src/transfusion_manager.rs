use std::sync::Arc;
use nullherz_dna::{SampleRegistry, LibraryDatabase};
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

    pub fn commit_breeding(&self, parent_a_id: u64, parent_b_id: u64, bias: f32, library: &LibraryDatabase) {
        if let (Some(parent_a), Some(parent_b)) = (self.sample_registry.get(parent_a_id), self.sample_registry.get(parent_b_id)) {
            // 1. Breed DNA
            let child_dna = nullherz_dna::transfuse_dna(&parent_a.metadata.dna, &parent_b.metadata.dna, bias);

            // 2. Interpolate Audio Buffers (Simple time-domain linear blend for now)
            let len = parent_a.buffer.len().min(parent_b.buffer.len());
            let mut child_buffer = Vec::with_capacity(len);
            for i in 0..len {
                child_buffer.push(parent_a.buffer[i] * (1.0 - bias) + parent_b.buffer[i] * bias);
            }

            // 3. Register child
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
            let child_id = (parent_a_id ^ parent_b_id).wrapping_add(now);
            let mut child_metadata = parent_a.metadata.clone();
            child_metadata.dna = child_dna;

            let buffer_arc = Arc::new(child_buffer);
            self.sample_registry.register_with_metadata(child_id, buffer_arc.clone(), child_metadata.clone());

            // 4. Save to Database
            let track = nullherz_dna::LibraryTrack {
                id: child_id,
                path: format!("breeding/child_{}.wav", child_id),
                title: format!("Child of {} x {}", parent_a_id, parent_b_id),
                artist: "AnaWaves Breeder".to_string(),
                metadata: child_metadata,
            };
            let _ = library.save_track(&track);
            println!("Breeding Commited: Created Child ID={}", child_id);
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
