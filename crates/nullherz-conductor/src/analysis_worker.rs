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

                // Perform BPM, Transient and Key Analysis
                metadata.bpm = self.detect_bpm(&sample.buffer);
                metadata.transients = Arc::new(self.detect_transients(&sample.buffer));
                metadata.peaks = Arc::new(self.calculate_peaks(&sample.buffer, 1024));
                metadata.root_key = self.detect_root_key(&sample.buffer);

                // Generate Sound DNA (AnaWaves Stage 1)
                metadata.dna = self.analyze_dna(&sample.buffer);

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

    fn detect_root_key(&self, buffer: &[f32]) -> Option<f32> {
        if buffer.is_empty() { return None; }

        let fft_size = 4096; // Higher resolution for key detection
        let fft = SimdFft::new(fft_size);
        let mut re = AlignedBuffer::new(fft_size);
        let mut im = AlignedBuffer::new(fft_size);

        let mut chromagram = [0.0f32; 12];
        let sample_rate = 44100.0;

        // Analyze up to 2 seconds or buffer length
        let max_samples = buffer.len().min(sample_rate as usize * 2);
        let num_windows = max_samples / fft_size;

        if num_windows == 0 { return None; }

        for w in 0..num_windows {
            let offset = w * fft_size;
            for j in 0..fft_size {
                re[j] = buffer[offset + j];
                im[j] = 0.0;
            }

            fft.process(&mut re, &mut im);

            // Map bins to semitones
            // freq = bin * SR / fft_size
            // semitone = 12 * log2(freq / 440) + 69
            for bin in 1..fft_size/2 {
                let freq = (bin as f32 * sample_rate) / fft_size as f32;
                if !(20.0..=2000.0).contains(&freq) { continue; } // Focus on fundamental range

                let mag = (re[bin]*re[bin] + im[bin]*im[bin]).sqrt();
                let semitone = 12.0 * (freq / 440.0).log2() + 69.0;
                let pitch_class = (semitone.round() as i32 % 12 + 12) % 12;
                chromagram[pitch_class as usize] += mag;
            }
        }

        let (best_note, &max_mag) = chromagram.iter().enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())?;

        if max_mag < 0.1 { return None; }

        Some(best_note as f32)
    }

    fn analyze_dna(&self, buffer: &[f32]) -> nullherz_traits::SoundDNA {
        let mut dna = nullherz_traits::SoundDNA::default();

        // 1. Spectral Personality Analysis
        let fft_size = 1024;
        let fft = SimdFft::new(fft_size);
        let mut re = AlignedBuffer::new(fft_size);
        let mut im = AlignedBuffer::new(fft_size);

        // Analyze first block for energy map
        if buffer.len() >= fft_size {
            for j in 0..fft_size {
                re[j] = buffer[j];
                im[j] = 0.0;
            }
            fft.process(&mut re, &mut im);

            // Map 512 bins to 64 energy bins
            let mut total_energy = 0.0;
            let mut magnitudes = [0.0f32; 512];
            for bin in 0..512 {
                magnitudes[bin] = (re[bin] * re[bin] + im[bin] * im[bin]).sqrt();
                total_energy += magnitudes[bin];
            }

            for i in 0..64 {
                let mut sum = 0.0;
                for k in 0..8 {
                    sum += magnitudes[i * 8 + k];
                }
                dna.spectral.energy_map[i] = if total_energy > 0.0 {
                    (sum / total_energy * 255.0 * 10.0).min(255.0) as u8
                } else { 0 };
            }

            // Harmonicity & Tilt (Advanced Stage 1)
            let mut tilt_sum = 0.0;
            for (bin, &mag) in magnitudes.iter().enumerate().skip(1).take(511) {
                let freq_norm = bin as f32 / 512.0;
                tilt_sum += mag * (1.0 - freq_norm); // Simplified tilt
            }
            dna.spectral.tilt = tilt_sum / total_energy.max(1e-6);
        }

        // 2. Rhythmic DNA (Onset Mask)
        let transients = self.detect_transients(buffer);
        if !transients.is_empty() {
            let total_len = buffer.len() as f32;
            for &t in &transients {
                let pos_norm = t as f32 / total_len;
                let step = (pos_norm * 64.0) as usize;
                if step < 64 {
                    let u64_idx = step / 16; // 4 u64s to cover 64 steps
                    let bit_idx = step % 16;
                    dna.rhythmic.onset_mask[u64_idx] |= 1 << bit_idx;
                }
            }
        }

        dna
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
