// Non-RT plane (stem separation worker thread pool): thread spawn/sleep are sanctioned here.
#![allow(clippy::disallowed_methods)]
#![allow(clippy::collapsible_if)]

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::path::PathBuf;
use parking_lot::Mutex;
use nullherz_traits::{
    SampleRegistry, StemClassification, SingleStemMetadata, StemSetMetadata,
    SoundDNA, MipWaveform, SampleMetadata, SampleBuffer, MmapBuffer,
};
use nullherz_dna::{LibraryDatabase, GeneticLibrary};
use audio_dsp::util::WaveformProcessor;
use audio_dsp::Filter;

pub struct StemExtractionWorker {
    sample_registry: Arc<dyn SampleRegistry>,
    library: Arc<Mutex<LibraryDatabase>>,
    pending_queue: Arc<Mutex<Vec<u64>>>,
    processed_ids: Arc<Mutex<std::collections::HashSet<u64>>>,
    stems_dir: PathBuf,
    is_running: Arc<std::sync::atomic::AtomicBool>,
}

impl StemExtractionWorker {
    pub fn new(
        sample_registry: Arc<dyn SampleRegistry>,
        library: Arc<Mutex<LibraryDatabase>>,
    ) -> Self {
        let stems_dir = PathBuf::from("library/stems");
        let _ = std::fs::create_dir_all(&stems_dir);
        Self {
            sample_registry,
            library,
            pending_queue: Arc::new(Mutex::new(Vec::new())),
            processed_ids: Arc::new(Mutex::new(std::collections::HashSet::new())),
            stems_dir,
            is_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn request_extraction(&self, track_id: u64) {
        let mut queue = self.pending_queue.lock();
        if !queue.contains(&track_id) && !self.processed_ids.lock().contains(&track_id) {
            queue.push(track_id);
        }
    }

    pub fn start(&self) {
        if self.is_running.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }

        let worker = self.clone();
        std::thread::spawn(move || {
            while worker.is_running.load(std::sync::atomic::Ordering::Relaxed) {
                let next_id = {
                    let mut q = worker.pending_queue.lock();
                    if q.is_empty() {
                        None
                    } else {
                        Some(q.remove(0))
                    }
                };

                if let Some(track_id) = next_id {
                    worker.process_track_stems(track_id);
                } else {
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        });
    }

    pub fn is_extraction_complete(&self, track_id: u64) -> bool {
        self.processed_ids.lock().contains(&track_id)
    }

    /// Process two-stage cascading demixing for a track
    fn process_track_stems(&self, track_id: u64) {
        println!("StemExtractionWorker: Starting cascading demixing for track_id={}", track_id);

        let track_opt = {
            let lib = self.library.lock();
            lib.get_track(track_id).ok().flatten()
        };

        let Some(_track) = track_opt else { return; };
        let sample_opt = self.sample_registry.get(track_id);
        let Some(sample) = sample_opt else { return; };

        let track_stems_dir = self.stems_dir.join(format!("{}", track_id));
        if let Err(e) = std::fs::create_dir_all(&track_stems_dir) {
            eprintln!("StemExtractionWorker: Failed to create stem directory: {}", e);
            return;
        }

        let total_samples = sample.buffer.len();
        let channels = (sample.metadata.channels as usize).max(1);
        let frames = total_samples / channels;

        let sr = sample.metadata.sample_rate.max(1) as f32;

        // Compute Harmonic-Percussive Source Separation (HPSS) transient vs harmonic envelopes per channel
        let mut percussive_weights = vec![vec![1.0f32; frames]; channels];
        let mut harmonic_weights = vec![vec![1.0f32; frames]; channels];

        let attack_coeff = (-1.0 / (sr * 0.005)).exp();  // 5ms attack
        let release_coeff = (-1.0 / (sr * 0.040)).exp(); // 40ms release
        let slow_coeff = (-1.0 / (sr * 0.150)).exp();    // 150ms slow RMS

        for ch in 0..channels {
            let start = ch * frames;
            let src_ch = &sample.buffer[start..start + frames];
            let mut fast_env = 0.0f32;
            let mut slow_env = 0.0f32;

            for i in 0..frames {
                let abs_v = src_ch[i].abs();
                if abs_v > fast_env {
                    fast_env = attack_coeff * fast_env + (1.0 - attack_coeff) * abs_v;
                } else {
                    fast_env = release_coeff * fast_env + (1.0 - release_coeff) * abs_v;
                }
                slow_env = slow_coeff * slow_env + (1.0 - slow_coeff) * abs_v;

                let transient_spike = (fast_env - slow_env).max(0.0);
                let p_ratio = (transient_spike * 4.0 / (slow_env + 1e-4)).clamp(0.0, 1.0);

                percussive_weights[ch][i] = p_ratio;
                harmonic_weights[ch][i] = (1.0 - p_ratio * 0.85).clamp(0.15, 1.0);
            }
        }

        // The 12 stem classifications with Linkwitz-Riley crossover frequencies and HPSS mode
        // Mode: 0 = Percussive, 1 = Harmonic, 2 = Mid (Center), 3 = Side (Stereo)
        let stem_configs = [
            (0usize, StemClassification::Kick, "kick", 20.0f32, 160.0f32, 0i32),
            (1usize, StemClassification::Snare, "snare", 150.0f32, 2500.0f32, 0i32),
            (2usize, StemClassification::Clap, "clap", 800.0f32, 6000.0f32, 0i32),
            (3usize, StemClassification::Hat, "hat", 4500.0f32, 20000.0f32, 0i32),
            (4usize, StemClassification::Percussion, "percussion", 300.0f32, 8000.0f32, 0i32),
            (5usize, StemClassification::Bass, "bass", 20.0f32, 280.0f32, 1i32),
            (6usize, StemClassification::LeadVocal, "lead_vocal", 300.0f32, 4000.0f32, 2i32),
            (7usize, StemClassification::BackingVocal, "backing_vocal", 350.0f32, 5000.0f32, 3i32),
            (8usize, StemClassification::Guitar, "guitar", 150.0f32, 3500.0f32, 1i32),
            (9usize, StemClassification::PianoKeys, "piano_keys", 200.0f32, 6000.0f32, 1i32),
            (10usize, StemClassification::SynthPad, "synth_pad", 80.0f32, 10000.0f32, 1i32),
            (11usize, StemClassification::BrassStrings, "brass_strings", 300.0f32, 8000.0f32, 1i32),
        ];

        let mut single_stems = Vec::new();

        for (idx, classif, name, low_cutoff, high_cutoff, hpss_mode) in stem_configs {
            let relative_filename = format!("stem_{:02}_{}.wav", idx + 1, name);
            let full_file_path = track_stems_dir.join(&relative_filename);

            let mut stem_samples = vec![0.0f32; total_samples];

            // Build Linkwitz-Riley high-pass and low-pass crossover biquads
            let hp_coeffs = audio_dsp::BiquadCoefficients::linkwitz_riley_hp(low_cutoff.max(10.0), sr);
            let lp_coeffs = audio_dsp::BiquadCoefficients::linkwitz_riley_lp(high_cutoff.min(sr * 0.48), sr);

            for ch in 0..channels {
                let start = ch * frames;
                let src_ch = &sample.buffer[start..start + frames];
                let dst_ch = &mut stem_samples[start..start + frames];

                let mut hp_filter = audio_dsp::BiquadFilter::new(hp_coeffs);
                let mut lp_filter = audio_dsp::BiquadFilter::new(lp_coeffs);

                for i in 0..frames {
                    let raw = src_ch[i];
                    let filtered = lp_filter.process_sample(hp_filter.process_sample(raw));

                    // Apply HPSS / Mid-Side spatial weighting
                    let weight = match hpss_mode {
                        0 => percussive_weights[ch][i],
                        1 => harmonic_weights[ch][i],
                        2 => harmonic_weights[ch][i], // Lead Vocal (Center)
                        3 => harmonic_weights[ch][i], // Backing Vocal (Side)
                        _ => 1.0,
                    };

                    dst_ch[i] = filtered * weight;
                }
            }

            // Mid-Side spatial isolation for Vocals (Lead Vocal = Center M, Backing Vocal = Side S)
            if channels >= 2 {
                if hpss_mode == 2 { // Lead Vocal -> Center Mid channel
                    let start_r = frames;
                    for i in 0..frames {
                        let l = stem_samples[i];
                        let r = stem_samples[start_r + i];
                        let mid = (l + r) * 0.5;
                        stem_samples[i] = mid;
                        stem_samples[start_r + i] = mid;
                    }
                } else if hpss_mode == 3 { // Backing Vocal -> Side channel
                    let start_r = frames;
                    for i in 0..frames {
                        let l = stem_samples[i];
                        let r = stem_samples[start_r + i];
                        let side = (l - r) * 0.5;
                        stem_samples[i] = side;
                        stem_samples[start_r + i] = -side;
                    }
                }
            }

            // Calculate peak_db and LUFS integrated
            let mut peak_val = 0.0f32;
            let mut sum_sq = 0.0f32;
            for &s in &stem_samples {
                let abs_s = s.abs();
                if abs_s > peak_val { peak_val = abs_s; }
                sum_sq += s * s;
            }
            let peak_db = 20.0 * peak_val.max(1e-5).log10();
            let rms_val = (sum_sq / stem_samples.len().max(1) as f32).sqrt();
            let lufs_integrated = 20.0 * rms_val.max(1e-5).log10() - 3.0;

            // Generate MIP waveform levels
            let mut peaks = Vec::with_capacity(frames / 128 + 1);
            for chunk in stem_samples.chunks(128 * channels) {
                let mut p = 0.0f32;
                for &s in chunk { if s.abs() > p { p = s.abs(); } }
                peaks.push(p);
            }
            let mip_levels = WaveformProcessor::generate_mip_levels(&peaks, 8);
            let mip_waveform = MipWaveform {
                levels: mip_levels.into_iter().map(Arc::new).collect(),
            };

            // Save planar float WAV to disk
            if let Ok(mut writer) = hound::WavWriter::create(
                &full_file_path,
                hound::WavSpec {
                    channels: channels as u16,
                    sample_rate: sample.metadata.sample_rate.max(1),
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                },
            ) {
                for f in 0..frames {
                    for c in 0..channels {
                        let val = stem_samples[c * frames + f];
                        let _ = writer.write_sample(val);
                    }
                }
                let _ = writer.finalize();
            }

            let relative_path = format!("library/stems/{}/{}", track_id, relative_filename);

            // Register stem audio buffer in memory-mapped buffer if file exists, or heap fallback
            let mmap_buffer = MmapBuffer::open(&full_file_path)
                .map(|m| SampleBuffer::Mmap(Arc::new(m)))
                .unwrap_or_else(|_| SampleBuffer::Heap(Arc::new(stem_samples.clone())));

            let stem_id = track_id.wrapping_add((idx as u64 + 1) * 10000);
            let mut stem_meta = SampleMetadata::new_empty();
            stem_meta.total_samples = frames as u64;
            stem_meta.channels = channels as u16;
            stem_meta.sample_rate = sample.metadata.sample_rate;
            stem_meta.mip_waveform = mip_waveform.clone();
            stem_meta.peaks = Arc::new(peaks);

            self.sample_registry.register_with_metadata(stem_id, mmap_buffer, Arc::new(stem_meta));

            let stem_dna = SoundDNA {
                schema_version: 7,
                feature_vector: [low_cutoff / sr, high_cutoff / sr, lufs_integrated, peak_db, 0.5, 0.5, 0.5, 0.5],
                ..SoundDNA::default()
            };

            single_stems.push(SingleStemMetadata {
                classification: classif,
                relative_path,
                lufs_integrated,
                peak_db,
                dna: stem_dna,
                mip_waveform,
            });
        }

        let created_at_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let stem_set = StemSetMetadata {
            track_id,
            tier: 12,
            stems: single_stems,
            created_at_timestamp,
        };

        // Update LibraryTrack in database
        {
            let lib = self.library.lock();
            if let Ok(Some(mut t)) = lib.get_track(track_id) {
                t.stems = Some(stem_set);
                let _ = lib.save_track(&t);
            }
        }

        self.processed_ids.lock().insert(track_id);
        println!("StemExtractionWorker: Completed 12-stem demixing for track_id={}", track_id);
    }
}

impl Clone for StemExtractionWorker {
    fn clone(&self) -> Self {
        Self {
            sample_registry: self.sample_registry.clone(),
            library: self.library.clone(),
            pending_queue: self.pending_queue.clone(),
            processed_ids: self.processed_ids.clone(),
            stems_dir: self.stems_dir.clone(),
            is_running: self.is_running.clone(),
        }
    }
}
