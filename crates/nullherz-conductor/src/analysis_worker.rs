use std::sync::Arc;
use nullherz_dna::{SampleRegistry, SampleMetadata};
use audio_dsp::TransientDetector;
use std::time::Duration;

pub struct AnalysisWorker {
    sample_registry: Arc<SampleRegistry>,
    processed_ids: std::collections::HashSet<u64>,
}

impl AnalysisWorker {
    pub fn new(sample_registry: Arc<SampleRegistry>) -> Self {
        Self {
            sample_registry,
            processed_ids: std::collections::HashSet::new(),
        }
    }

    pub fn start(mut self) {
        std::thread::spawn(move || {
            loop {
                self.run_once();
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }

    fn run_once(&mut self) {
        let ids = self.sample_registry.list_ids();
        for id in ids {
            if self.processed_ids.contains(&id) { continue; }

            if let Some(sample) = self.sample_registry.get(id) {
                println!("AnalysisWorker: Analyzing sample ID={}", id);
                let mut metadata = sample.metadata.clone();

                // Perform BPM and Transient Analysis
                metadata.bpm = self.detect_bpm(&sample.buffer);
                metadata.transients = Arc::new(self.detect_transients(&sample.buffer));

                // Update registry with enriched metadata
                self.sample_registry.register_with_metadata(id, sample.buffer.clone(), metadata);
                self.processed_ids.insert(id);
                println!("AnalysisWorker: Enriched metadata for ID={}", id);
            }
        }
    }

    fn detect_bpm(&self, buffer: &[f32]) -> f32 {
        if buffer.is_empty() { return 0.0; }
        // Dummy implementation for Alpha
        128.0
    }

    fn detect_transients(&self, buffer: &[f32]) -> Vec<u64> {
        let mut transients = Vec::new();
        let mut detector = TransientDetector::new(1024, 0.2);
        let block_size = 1024;

        for i in (0..buffer.len()).step_by(block_size) {
            let end = (i + block_size).min(buffer.len());
            let chunk = &buffer[i..end];
            if chunk.len() < block_size { break; }

            let im = vec![0.0; block_size];
            if detector.is_transient(chunk, &im) {
                transients.push(i as u64);
            }
        }
        transients
    }
}
