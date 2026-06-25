use std::sync::Arc;
use nullherz_dna::SampleRegistry;
use audio_dsp::{TransientDetector, SimdFft, AlignedBuffer};
use std::time::Duration;

pub struct AnalysisWorker {
    sample_registry: Arc<SampleRegistry>,
    library: Option<Arc<std::sync::Mutex<nullherz_dna::LibraryDatabase>>>,
    processed_ids: std::collections::HashSet<u64>,
}

impl AnalysisWorker {
    pub fn new(sample_registry: Arc<SampleRegistry>) -> Self {
        Self {
            sample_registry,
            library: None,
            processed_ids: std::collections::HashSet::new(),
        }
    }

    pub fn with_library(mut self, library: Arc<std::sync::Mutex<nullherz_dna::LibraryDatabase>>) -> Self {
        self.library = Some(library);
        self
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
                metadata.peaks = Arc::new(self.calculate_peaks(&sample.buffer, 1024));

                // Update registry with enriched metadata
                self.sample_registry.register_with_metadata(id, sample.buffer.clone(), metadata.clone());

                // Update Library Database (redb)
                if let Some(ref lib_mutex) = self.library {
                    let lib = lib_mutex.lock().unwrap();
                    if let Ok(Some(mut track)) = lib.get_track(id) {
                        track.metadata = metadata;
                        let _ = lib.save_track(&track);
                    }
                }

                self.processed_ids.insert(id);
                println!("AnalysisWorker: Enriched metadata for ID={}", id);
            }
        }
    }

    fn calculate_peaks(&self, buffer: &[f32], target_width: usize) -> Vec<f32> {
        if buffer.is_empty() { return Vec::new(); }
        let mut peaks = Vec::with_capacity(target_width);
        let chunk_size = buffer.len() / target_width;
        if chunk_size == 0 { return buffer.to_vec(); }

        for i in 0..target_width {
            let start = i * chunk_size;
            let end = (start + chunk_size).min(buffer.len());
            let mut max_val = 0.0f32;
            for &s in &buffer[start..end] {
                if s.abs() > max_val { max_val = s.abs(); }
            }
            peaks.push(max_val);
        }
        peaks
    }

    fn detect_bpm(&self, buffer: &[f32]) -> f32 {
        if buffer.is_empty() { return 0.0; }
        let transients = self.detect_transients(buffer);
        if transients.len() < 4 { return 128.0; }

        let mut intervals = Vec::new();
        for i in 1..transients.len() {
            let diff = transients[i] - transients[i-1];
            if diff > 5000 { // Ignore very close transients
                intervals.push(diff);
            }
        }

        if intervals.is_empty() { return 128.0; }

        // Simple histogram for common intervals
        let mut histogram: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        for &interval in &intervals {
            // Group by ~5ms buckets (220 samples at 44.1kHz)
            let bucket = (interval / 220) * 220;
            *histogram.entry(bucket).or_default() += 1;
        }

        let best_interval = histogram.into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(bucket, _)| bucket)
            .unwrap_or(22050); // Default to 120bpm if nothing found

        let bpm = (44100.0 * 60.0) / best_interval as f32;

        // Standardize to common ranges
        let mut final_bpm = bpm;
        while final_bpm < 70.0 { final_bpm *= 2.0; }
        while final_bpm > 180.0 { final_bpm /= 2.0; }

        final_bpm
    }

    fn detect_transients(&self, buffer: &[f32]) -> Vec<u64> {
        let mut transients = Vec::new();
        let fft_size = 1024;
        let hop_size = 512;
        let mut detector = TransientDetector::new(fft_size, 0.15); // Slightly more sensitive
        let fft = SimdFft::new(fft_size);

        let mut re = AlignedBuffer::new(fft_size);
        let mut im = AlignedBuffer::new(fft_size);

        for i in (0..buffer.len().saturating_sub(fft_size)).step_by(hop_size) {
            // Prepare chunk with Hann window or similar?
            // For now just copy and let TransientDetector handle it.
            for j in 0..fft_size {
                re[j] = buffer[i + j];
                im[j] = 0.0;
            }

            fft.process(&mut re, &mut im);

            if detector.is_transient(&re, &im) {
                transients.push(i as u64);
            }
        }
        transients
    }
}
