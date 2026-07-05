use audio_dsp::{TransientDetector, SimdFft, AlignedBuffer};
use std::sync::Arc;

pub struct AnalysisKernel {
    fft: SimdFft,
    re: AlignedBuffer,
    im: AlignedBuffer,
    transient_detector: TransientDetector,
    sample_rate: f32,
}

impl AnalysisKernel {
    pub fn new(sample_rate: f32) -> Self {
        let fft_size = 1024;
        Self {
            fft: SimdFft::new(fft_size),
            re: AlignedBuffer::new(fft_size),
            im: AlignedBuffer::new(fft_size),
            transient_detector: TransientDetector::new(fft_size, 0.15),
            sample_rate,
        }
    }

    pub fn analyze(&mut self, buffer: &[f32]) -> (nullherz_traits::SampleMetadata, nullherz_traits::SoundDNA) {
        let mut metadata = nullherz_traits::SampleMetadata::new_empty();
        let mut dna = nullherz_traits::SoundDNA::default();

        // 1. Detect Transients
        let transients = self.detect_transients(buffer);
        metadata.transients = Arc::new(transients.clone());

        // 2. Detect BPM
        metadata.bpm = self.detect_bpm_from_transients(&transients);

        // 2b. Calculate Peaks
        metadata.peaks = Arc::new(self.calculate_peaks(buffer, 1024));

        // 3. DNA Analysis (Spectral)
        self.analyze_spectral(buffer, &mut dna);

        // 4. DNA Analysis (Rhythmic)
        self.analyze_rhythmic(&transients, buffer.len(), metadata.bpm, &mut dna);

        // 5. DNA Analysis (Spatial)
        self.analyze_spatial(buffer, &transients, &mut dna);

        // 6. Detect Root Key
        metadata.root_key = self.detect_root_key(buffer);

        (metadata, dna)
    }

    fn detect_transients(&mut self, buffer: &[f32]) -> Vec<u64> {
        let mut transients = Vec::new();
        let fft_size = 1024;
        let hop_size = 512;

        for i in (0..buffer.len().saturating_sub(fft_size)).step_by(hop_size) {
            self.re.fill(0.0);
            self.im.fill(0.0);
            let len = (buffer.len() - i).min(fft_size);
            self.re[..len].copy_from_slice(&buffer[i..i+len]);

            self.fft.process(&mut self.re, &mut self.im);

            if self.transient_detector.is_transient(&self.re, &self.im) {
                transients.push(i as u64);
            }
        }
        transients
    }

    fn detect_bpm_from_transients(&self, transients: &[u64]) -> f32 {
        if transients.len() < 4 { return 128.0; }

        let mut intervals = Vec::new();
        for i in 1..transients.len() {
            let diff = transients[i] - transients[i-1];
            if diff > 5000 {
                intervals.push(diff);
            }
        }

        if intervals.is_empty() { return 128.0; }

        let mut histogram: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        for &interval in &intervals {
            let bucket = (interval / 220) * 220;
            *histogram.entry(bucket).or_default() += 1;
        }

        let best_interval = histogram.into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(bucket, _)| bucket)
            .unwrap_or((self.sample_rate / 2.0) as u64);

        let bpm = (self.sample_rate * 60.0) / best_interval as f32;
        let mut final_bpm = bpm;
        if final_bpm > 10.0 {
            while final_bpm < 70.0 { final_bpm *= 2.0; }
            while final_bpm > 175.0 { final_bpm /= 2.0; }
        } else {
            final_bpm = 120.0;
        }
        final_bpm
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

    fn analyze_spectral(&mut self, buffer: &[f32], dna: &mut nullherz_traits::SoundDNA) {
        let fft_size = 1024;
        if buffer.len() < fft_size { return; }

        self.re.fill(0.0);
        self.im.fill(0.0);
        self.re[..fft_size].copy_from_slice(&buffer[..fft_size]);

        self.fft.process(&mut self.re, &mut self.im);

        let mut total_energy = 0.0;
        let mut magnitudes = [0.0f32; 512];
        for bin in 0..512 {
            magnitudes[bin] = (self.re[bin] * self.re[bin] + self.im[bin] * self.im[bin]).sqrt();
            total_energy += magnitudes[bin];
        }

        for i in 0..16 {
            let mut sum = 0.0;
            for k in 0..32 {
                sum += magnitudes[i * 32 + k];
            }
            dna.spectral.latent_space[i] = if total_energy > 0.0 {
                (sum / total_energy * 10.0).min(1.0)
            } else { 0.0 };
        }

        let mut tilt_sum = 0.0;
        for (bin, &mag) in magnitudes.iter().enumerate().skip(1).take(511) {
            let freq_norm = bin as f32 / 512.0;
            tilt_sum += mag * (1.0 - freq_norm);
        }
        dna.spectral.tilt = tilt_sum / total_energy.max(1e-6);
    }

    fn analyze_rhythmic(&self, transients: &[u64], buffer_len: usize, bpm: f32, dna: &mut nullherz_traits::SoundDNA) {
        if transients.is_empty() { return; }
        let total_len = buffer_len as f32;
        let mut steps = [false; 64];
        for &t in transients {
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

        let mut syncopation = 0.0;
        let weights = [0, 4, 2, 4, 1, 4, 2, 4, 0, 4, 2, 4, 1, 4, 2, 4];
        for i in 0..64 {
            if steps[i] {
                let next = (i + 1) % 64;
                if !steps[next] {
                    syncopation += weights[i % 16] as f32;
                }
            }
        }
        dna.rhythmic.syncopation_index = syncopation / 64.0;

        if bpm > 10.0 {
            let samples_per_beat = (self.sample_rate as f64 * 60.0) / bpm as f64;
            let samples_per_step = samples_per_beat / 4.0;
            let mut deviations: [Vec<i16>; 12] = Default::default();
            for &t in transients {
                let step_idx = (t as f64 / samples_per_step).round();
                let grid_pos = step_idx * samples_per_step;
                let deviation_ms = ((t as f64 - grid_pos) / self.sample_rate as f64) * 1000.0;
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

    fn detect_root_key(&mut self, buffer: &[f32]) -> Option<f32> {
        if buffer.is_empty() { return None; }

        let fft_size = 4096;
        let fft = SimdFft::new(fft_size);
        let mut re = AlignedBuffer::new(fft_size);
        let mut im = AlignedBuffer::new(fft_size);

        let mut chromagram = [0.0f32; 12];
        let max_samples = buffer.len().min(self.sample_rate as usize * 2);
        let num_windows = max_samples / fft_size;

        if num_windows == 0 { return None; }

        for w in 0..num_windows {
            let offset = w * fft_size;
            re.fill(0.0);
            im.fill(0.0);
            re[..fft_size].copy_from_slice(&buffer[offset..offset+fft_size]);

            fft.process(&mut re, &mut im);

            for bin in 1..fft_size/2 {
                let freq = (bin as f32 * self.sample_rate) / fft_size as f32;
                if !(20.0..=2000.0).contains(&freq) { continue; }

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

    fn analyze_spatial(&self, buffer: &[f32], transients: &[u64], dna: &mut nullherz_traits::SoundDNA) {
        if let Some(&last_t) = transients.last() {
            let tail = &buffer[last_t as usize..];
            let mut rms_max = 0.0001f32;
            let mut rms_end = 0.0001f32;

            for chunk in tail.chunks(512).take(10) {
                let rms = (chunk.iter().map(|x| x*x).sum::<f32>() / chunk.len() as f32).sqrt();
                if rms > rms_max { rms_max = rms; }
            }

            if tail.len() > 1024 {
                 let end_chunk = &tail[tail.len()-1024..];
                 rms_end = (end_chunk.iter().map(|x| x*x).sum::<f32>() / 1024.0).sqrt();
            }

            let decay_db = 20.0 * (rms_end / rms_max).log10();
            if decay_db < -10.0 {
                 dna.spatial.room_size = (decay_db.abs() / 60.0).clamp(0.0, 1.0);
            }
        }
    }
}
