use std::sync::Arc;
use nullherz_dna::SampleRegistry;
use audio_dsp::{TransientDetector, SimdFft, AlignedBuffer};
use std::time::Duration;
use rayon::prelude::*;

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
        let unprocessed_ids: Vec<u64> = ids.into_iter()
            .filter(|id| !self.processed_ids.contains(id))
            .collect();

        if unprocessed_ids.is_empty() { return; }

        println!("AnalysisWorker: Processing {} new samples in batch", unprocessed_ids.len());

        let results: Vec<(u64, nullherz_traits::SampleMetadata, Arc<Vec<f32>>)> = unprocessed_ids.into_par_iter()
            .filter_map(|id| {
                let sample = self.sample_registry.get(id)?;
                let mut metadata = sample.metadata.clone();
                let sample_rate = 44100.0;

                metadata.bpm = self.detect_bpm(&sample.buffer, sample_rate);
                metadata.transients = Arc::new(self.detect_transients(&sample.buffer, sample_rate));
                metadata.peaks = Arc::new(self.calculate_peaks(&sample.buffer, 1024));
                metadata.root_key = self.detect_root_key(&sample.buffer, sample_rate);
                metadata.dna = self.analyze_dna(&sample.buffer, metadata.bpm, sample_rate);
                self.analyze_spatial(&sample.buffer, &mut metadata.dna, sample_rate);

                Some((id, metadata, sample.buffer))
            }).collect();

        for (id, metadata, buffer) in results {
            self.sample_registry.register_with_metadata(id, buffer, metadata.clone());

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

    fn detect_bpm(&self, buffer: &[f32], sample_rate: f32) -> f32 {
        if buffer.is_empty() { return 0.0; }
        let transients = self.detect_transients(buffer, sample_rate);
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

        // Smoothing: Consolidate double/half intervals
        let mut smoothed_histogram: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        let keys: Vec<u64> = histogram.keys().cloned().collect();
        for &k in &keys {
            let count = histogram[&k];
            let mut matched_key = None;

            // Find an existing bucket that is related (harmonic/sub-harmonic)
            for &sk in smoothed_histogram.keys() {
                let ratio = k as f32 / sk as f32;
                if (ratio - 1.0).abs() < 0.05 || (ratio - 2.0).abs() < 0.05 || (ratio - 0.5).abs() < 0.05 {
                    matched_key = Some(sk);
                    break;
                }
            }

            if let Some(sk) = matched_key {
                *smoothed_histogram.get_mut(&sk).unwrap() += count;
            } else {
                smoothed_histogram.insert(k, count);
            }
        }

        let best_interval = smoothed_histogram.into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(bucket, _)| bucket)
            .unwrap_or((sample_rate / 2.0) as u64); // Default to 120bpm if nothing found

        let bpm = (sample_rate * 60.0) / best_interval as f32;

        // Standardize to common dance music ranges (70-175 BPM)
        let mut final_bpm = bpm;
        if final_bpm > 10.0 {
            while final_bpm < 70.0 { final_bpm *= 2.0; }
            while final_bpm > 175.0 { final_bpm /= 2.0; }
        } else {
            final_bpm = 120.0; // Default fallback for unmeasurable signals
        }

        final_bpm
    }

    fn detect_root_key(&self, buffer: &[f32], sample_rate: f32) -> Option<f32> {
        if buffer.is_empty() { return None; }

        let fft_size = 4096; // Higher resolution for key detection
        let fft = SimdFft::new(fft_size);
        let mut re = AlignedBuffer::new(fft_size);
        let mut im = AlignedBuffer::new(fft_size);

        let mut chromagram = [0.0f32; 12];

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

    fn analyze_dna(&self, buffer: &[f32], bpm: f32, sample_rate: f32) -> nullherz_traits::SoundDNA {
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

        // 2. Rhythmic DNA (Onset Mask & Syncopation)
        let transients = self.detect_transients(buffer, sample_rate);
        if !transients.is_empty() {
            let total_len = buffer.len() as f32;
            let mut steps = [false; 64];
            for &t in &transients {
                let pos_norm = t as f32 / total_len;
                let step = (pos_norm * 64.0) as usize;
                if step < 64 {
                    steps[step] = true;
                    let u64_idx = step / 64;
                    let bit_idx = step % 64;
                    if u64_idx < 4 {
                        dna.rhythmic.onset_mask[u64_idx] |= 1 << bit_idx;
                    }
                }
            }

            // Syncopation Index Calculation (LHL algorithm approximation)
            let mut syncopation = 0.0;
            let weights = [0, 4, 2, 4, 1, 4, 2, 4, 0, 4, 2, 4, 1, 4, 2, 4]; // Simplified 16-step weights
            for i in 0..64 {
                if steps[i] {
                    let next = (i + 1) % 64;
                    if !steps[next] {
                        syncopation += weights[i % 16] as f32;
                    }
                }
            }
            dna.rhythmic.syncopation_index = syncopation / 64.0;

            // 3. Micro-timing Profile Detection
            // Calculate deviation from a perfect 16th-note grid
            if bpm > 10.0 {
                let samples_per_beat = (sample_rate as f64 * 60.0) / bpm as f64;
                let samples_per_step = samples_per_beat / 4.0; // 16th note

                let mut deviations: [Vec<i16>; 12] = Default::default();
                for &t in &transients {
                    let step_idx = (t as f64 / samples_per_step).round();
                    let grid_pos = step_idx * samples_per_step;
                    let deviation_ms = ((t as f64 - grid_pos) / sample_rate as f64) * 1000.0;

                    let profile_idx = step_idx as usize % 12;
                    deviations[profile_idx].push(deviation_ms as i16);
                }

                for i in 0..12 {
                    if !deviations[i].is_empty() {
                        let sum: i32 = deviations[i].iter().map(|&x| x as i32).sum();
                        dna.rhythmic.micro_timing[i] = (sum / deviations[i].len() as i32) as i16;
                    }
                }
            }
        }

        dna
    }

    fn analyze_spatial(&self, buffer: &[f32], dna: &mut nullherz_traits::SoundDNA, sample_rate: f32) {
        if buffer.len() < (sample_rate as usize) { return; }

        // 1. Estimate Stereo Width (assuming buffer might be interleaved or we just check energy)
        // Simplified: Heuristic based on high-frequency variance if we had multiple channels.
        // For mono buffer, we'll assume width 1.0.

        // 2. Reverb Tail Estimation (T60 approximation)
        // Look for the decay after the last significant transient.
        let transients = self.detect_transients(buffer, sample_rate);
        if let Some(&last_t) = transients.last() {
            let tail = &buffer[last_t as usize..];
            let mut rms_max = 0.0001f32;
            let mut rms_end = 0.0001f32;

            // Max energy in tail
            for chunk in tail.chunks(512).take(10) {
                let rms = (chunk.iter().map(|x| x*x).sum::<f32>() / chunk.len() as f32).sqrt();
                if rms > rms_max { rms_max = rms; }
            }

            // End energy
            if tail.len() > 1024 {
                 let end_chunk = &tail[tail.len()-1024..];
                 rms_end = (end_chunk.iter().map(|x| x*x).sum::<f32>() / 1024.0).sqrt();
            }

            let decay_db = 20.0 * (rms_end / rms_max).log10();
            if decay_db < -10.0 {
                 dna.spatial.room_size = (decay_db.abs() / 60.0).clamp(0.0, 1.0);
            }

            // 3. Early Reflections (ER) pattern
            // Find secondary peaks in the first 50ms after transient
            let er_window_samples = (sample_rate * 0.05) as usize;
            let er_zone = &tail[..er_window_samples.min(tail.len())];

            let mut tap_idx = 0;
            for (i, chunk) in er_zone.chunks(64).enumerate() {
                if tap_idx >= 8 { break; }
                let rms = (chunk.iter().map(|x| x*x).sum::<f32>() / chunk.len() as f32).sqrt();
                if rms > rms_max * 0.1 { // Detect distinct reflection
                    dna.spatial.er_taps[tap_idx] = (i * 64) as f32 / sample_rate * 1000.0;
                    dna.spatial.er_gains[tap_idx] = (rms / rms_max).min(1.0);
                    tap_idx += 1;
                }
            }
        }
    }

    fn detect_transients(&self, buffer: &[f32], _sample_rate: f32) -> Vec<u64> {
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
