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

    pub fn rhythmic_transfusion(&self, dna_a: &nullherz_traits::RhythmicDNA, dna_b: &nullherz_traits::RhythmicDNA, bias: f32) -> nullherz_traits::RhythmicDNA {
        let mut child = nullherz_traits::RhythmicDNA::default();
        let inv_bias = 1.0 - bias;

        for i in 0..4 {
            let mask_a = dna_a.onset_mask[i];
            let mask_b = dna_b.onset_mask[i];
            let mut child_mask = 0u64;
            for bit in 0..64 {
                let bit_a = (mask_a >> bit) & 1;
                let bit_b = (mask_b >> bit) & 1;
                let prob = if bit_a == 1 && bit_b == 1 { 1.0 }
                          else if bit_a == 1 { inv_bias }
                          else if bit_b == 1 { bias }
                          else { 0.0 };

                // Deterministic pseudo-randomness based on bit position for consistent results
                let seed = (i as u32).wrapping_mul(64).wrapping_add(bit as u32);
                let rand_val = (seed.wrapping_mul(1103515245).wrapping_add(12345) as f32) / 4294967295.0;

                if rand_val < prob {
                    child_mask |= 1 << bit;
                }
            }
            child.onset_mask[i] = child_mask;
        }

        child.syncopation_index = dna_a.syncopation_index * inv_bias + dna_b.syncopation_index * bias;
        for i in 0..12 {
            child.micro_timing[i] = (dna_a.micro_timing[i] as f32 * inv_bias + dna_b.micro_timing[i] as f32 * bias) as i16;
        }

        child
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

    pub fn commit_chaotic_breeding(&self, parent_a_id: u64, parent_b_id: u64, bias: f32, chaotic_strength: f32, library: &LibraryDatabase) {
        if let (Some(parent_a), Some(parent_b)) = (self.sample_registry.get(parent_a_id), self.sample_registry.get(parent_b_id)) {
            // 1. Breed DNA Chaotically
            let child_dna = nullherz_dna::chaotic_transfuse_dna(&parent_a.metadata.dna, &parent_b.metadata.dna, bias, chaotic_strength);

            // 2. Interpolate Audio Buffers
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
                path: format!("breeding/chaotic_child_{}.wav", child_id),
                title: format!("Chaotic Child of {} x {}", parent_a_id, parent_b_id),
                artist: "AnaWaves Breeder".to_string(),
                metadata: child_metadata,
            };
            let _ = library.save_track(&track);
            println!("Chaotic Breeding Commited: Created Child ID={}", child_id);
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
                mip_waveform: nullherz_traits::MipWaveform::default(),
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
