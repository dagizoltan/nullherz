// Non-RT plane (library scan worker thread): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
#![allow(clippy::collapsible_if)]
use nullherz_traits::SampleRegistry;
use std::sync::Arc;
use nullherz_dna::{ LibraryDatabase, LibraryTrack, GeneticLibrary};
use nullherz_traits::SampleMetadata;
use std::path::Path;
use std::time::Duration;

pub struct FolderMonitor {
    sample_registry: Arc<dyn SampleRegistry>,
    library: Arc<parking_lot::Mutex<LibraryDatabase>>,
    /// Ids this monitor has already decoded and handed to the registry.
    ///
    /// The scan used to dedupe purely on registry residency ("already
    /// registered? skip"). That conflated two different questions — *is this
    /// audio resident* and *have we already ingested this file* — and they
    /// stopped being the same thing the moment residency became reclaimable:
    /// the reaper would release a track, the next 10-second sweep would see a
    /// registry miss, re-decode the entire file, and the reaper would release
    /// it again. A permanent decode/evict treadmill on the largest files in the
    /// library, plus the memory spike the residency check was added to prevent.
    ///
    /// Session-scoped, matching the previous behaviour exactly: a content change
    /// at a known path is still only picked up on the next app start.
    scanned: Arc<parking_lot::Mutex<std::collections::HashSet<u64>>>,
}

impl Clone for FolderMonitor {
    fn clone(&self) -> Self {
        Self {
            sample_registry: self.sample_registry.clone(),
            library: self.library.clone(),
            scanned: self.scanned.clone(),
        }
    }
}

impl FolderMonitor {
    pub fn new(sample_registry: Arc<dyn SampleRegistry>, library: Arc<parking_lot::Mutex<LibraryDatabase>>) -> Self {
        Self {
            sample_registry,
            library,
            scanned: Arc::new(parking_lot::Mutex::new(std::collections::HashSet::new())),
        }
    }

    pub fn scan_folder(&self, path: &str) {
        let path_str = path.to_string();
        let self_cloned = self.clone();
        std::thread::spawn(move || {
            self_cloned.scan_folder_sync(&path_str);
        });
    }

    pub fn scan_folder_sync(&self, path: &str) {
        let path_obj = Path::new(path);
        if !path_obj.is_dir() { return; }

        let entries: Vec<_> = walkdir::WalkDir::new(path_obj)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                if let Some(ext) = e.path().extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    ext == "wav" || ext == "flac" || ext == "mp3" || ext == "ogg"
                } else {
                    false
                }
            })
            .collect();

        // Decode SEQUENTIALLY, not with `into_par_iter()`. Each decode holds a
        // whole file (plus a decode-time intermediate) in memory; decoding
        // every file at once fanned the peak out to (file size x core count)
        // and pinned every core, starving the in-process UI thread — a
        // memory/CPU spike that froze the app on a library of large files. One
        // at a time bounds the transient to a single decode; this is a
        // background scan (its own thread), so throughput is not critical, and
        // the registry skip in `load_and_register` means it only does real work
        // for genuinely new files.
        for entry in entries {
            if let Some(path_str) = entry.path().to_str() {
                self.load_and_register(path_str);
            }
        }
    }

    fn load_and_register(&self, path: &str) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        let id = hasher.finish();

        // Already decoded and in the (in-memory) registry this session — there
        // is nothing to do, so skip the decode ENTIRELY. `start_auto_scan`
        // reruns this scan every 10 s to pick up NEW files; the decode used to
        // run BEFORE any existence check, so every cycle re-read and
        // re-allocated the whole library in parallel while the previous copies
        // were still live in the registry — memory periodically doubled, and
        // with large files that exhausted RAM and froze the app soon after
        // startup. The `get` here is the registry's lock-free reader (cheap).
        // (A content change to an already-loaded file is now only picked up on
        // the next app start; detecting it live without re-decoding would need
        // an mtime/size check — a follow-up, not worth re-freezing for.)
        // Resident OR already ingested this session. The second clause is what
        // keeps this from re-decoding a track the reaper legitimately released.
        if self.sample_registry.get(id).is_some() || self.scanned.lock().contains(&id) {
            return;
        }

        let decoded = decode_audio_file(path);
        // Record before registering: if anything below bails out early we still
        // must not decode this file again on the next sweep.
        self.scanned.lock().insert(id);

        // The SampleRegistry is in-memory and empty on EVERY boot; the library
        // is persistent. A library hit must still hydrate the registry, or every
        // deck load after the first-ever scan silently resolves to no buffer
        // (the "eternal silence on second boot" bug). A content change at the
        // same path (regenerated demo tracks) must fall through and re-analyze,
        // or stale metadata/waveforms live forever.
        let existing = { let lib = self.library.lock(); lib.get_track(id).ok().flatten() };
        if let Some(track) = existing {
            // `sample_rate` is part of the freshness check, not just frames and
            // channels. Rows written before the field existed default to 44.1 kHz,
            // so a genuinely 48 kHz file would otherwise match on frames+channels
            // and be hydrated with a rate that transposes it — and stay that way
            // forever. Including it here makes an existing library self-heal on
            // the next scan.
            if track.metadata.total_samples == decoded.frames as u64
                && track.metadata.channels as usize == decoded.channels
                && track.metadata.sample_rate == decoded.sample_rate
            {
                self.sample_registry.register_with_metadata(id, decoded.samples, track.metadata.clone());
                println!("FolderMonitor: Hydrated registry for {}", path);
                return;
            }
            println!("FolderMonitor: Content changed for {}; re-analyzing.", path);
        }
        let lib = self.library.lock();

        let mut metadata = SampleMetadata::new_empty();
        metadata.total_samples = decoded.frames as u64;
        metadata.channels = decoded.channels as u16;
        metadata.sample_rate = decoded.sample_rate;

        let track = LibraryTrack {
            id,
            path: path.to_string(),
            title: Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string()),
            artist: "Unknown".to_string(),
            album: "Unknown".to_string(),
            genre: "Unknown".to_string(),
            energy_level: 0.5,
            metadata: Arc::new(metadata),
        };

        let _ = lib.save_track(&track);
        self.sample_registry.register_with_metadata(id, decoded.samples, track.metadata.clone());
        println!("FolderMonitor: Registered {}", path);
    }

    pub fn start_auto_scan(self, path: String) {
        std::thread::spawn(move || {
            loop {
                self.scan_folder_sync(&path);
                std::thread::sleep(Duration::from_secs(10));
            }
        });
    }
}


/// A decoded audio file in PLANAR layout: channel 0 occupies `samples[0..frames]`,
/// channel 1 `samples[frames..2*frames]`, and so on.
///
/// Planar rather than interleaved is load-bearing, not a style choice. The
/// sampler voice interpolates across four CONSECUTIVE buffer elements (via a
/// SIMD 4-wide load), so interleaved data would have it interpolating across
/// L,R,L,R — mixing channels together — and there is no stride-aware form of
/// that load. Keeping each channel contiguous leaves the interpolator and its
/// SIMD path correct, with a channel selected by offset alone.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub samples: Arc<Vec<f32>>,
    /// Frames PER CHANNEL (not total samples).
    pub frames: usize,
    pub channels: usize,
    /// Sample rate of the SOURCE FILE, which is not necessarily the device rate.
    ///
    /// This used to be discarded: the decoder read `codec_params.sample_rate`
    /// and threw it away, so every file was implicitly treated as 44.1 kHz.
    /// Since the sampler advances one frame per output frame, that transposed
    /// anything else — a 48 kHz file played 1.5 semitones flat, 96 kHz an octave
    /// down. Carrying it lets the sampler compensate and lets frame-denominated
    /// metadata (cues, beat grid) be interpreted against the right clock.
    pub sample_rate: u32,
}

impl DecodedAudio {
    /// Samples for one channel, clamped to what actually exists so a caller
    /// asking for a channel the file does not have gets channel 0 rather than
    /// a panic or silence.
    pub fn channel(&self, channel: usize) -> &[f32] {
        let c = if channel < self.channels { channel } else { 0 };
        let start = c * self.frames;
        &self.samples[start..start + self.frames]
    }

    fn silent_fallback() -> Self {
        let frames = 44100 * 5;
        Self { samples: Arc::new(vec![0.0f32; frames]), frames, channels: 1, sample_rate: 44100 }
    }
}

/// Decode any supported audio file to a planar f32 buffer.
/// Non-RT; used by the scanner and by on-demand deck-load hydration.
pub fn decode_audio_file(path: &str) -> DecodedAudio {
    decode_audio_file_with_progress(path, |_| {})
}

/// Decode any supported audio file to a planar f32 buffer, with progress updates.
pub fn decode_audio_file_with_progress<F>(path: &str, mut progress_cb: F) -> DecodedAudio
where
    F: FnMut(f32) + Send + 'static,
{
    use symphonia::core::audio::Signal;
    if let Ok(file) = std::fs::File::open(path) {
        let mss = symphonia::core::io::MediaSourceStream::new(Box::new(file), Default::default());
        let hint = symphonia::core::probe::Hint::new();
        if let Ok(mut probed) = symphonia::default::get_probe().format(&hint, mss, &Default::default(), &Default::default()) {
            if let Some(track) = probed.format.default_track() {
                if let Ok(mut decoder) = symphonia::default::get_codecs().make(&track.codec_params, &Default::default()) {
                    // Accumulate per channel, then concatenate: packets arrive
                    // in chunks, so channels can only be laid out contiguously
                    // once the whole file is known.
                    let total_frames = track.codec_params.n_frames;
                    // The source rate, which the old code read and discarded.
                    // Fall back to 44.1 kHz only if the container genuinely does
                    // not declare one.
                    let source_rate = track.codec_params.sample_rate.unwrap_or(nullherz_traits::LEGACY_SOURCE_SAMPLE_RATE);
                    let mut decoded_frames = 0u64;
                    let mut planes: Vec<Vec<f32>> = Vec::new();
                    let mut sample_buf = None;
                    while let Ok(packet) = probed.format.next_packet() {
                        if let Ok(decoded) = decoder.decode(&packet) {
                            let buf = sample_buf.get_or_insert_with(|| symphonia::core::audio::AudioBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec()));
                            decoded.convert(buf);
                            let chan_len = buf.frames();
                            let num_chans = buf.spec().channels.count();
                            if planes.len() < num_chans {
                                planes.resize(num_chans, Vec::new());
                            }
                            for (c, plane) in planes.iter_mut().enumerate().take(num_chans) {
                                plane.extend_from_slice(&buf.chan(c)[..chan_len]);
                            }

                            decoded_frames += chan_len as u64;
                            if let Some(total) = total_frames {
                                if total > 0 {
                                    let progress = (decoded_frames as f32 / total as f32).clamp(0.0, 1.0);
                                    progress_cb(progress);
                                }
                            }
                        }
                    }

                    if planes.is_empty() || planes[0].is_empty() {
                        eprintln!("decode_audio_file: decoded no audio from: {}", path);
                        return DecodedAudio::silent_fallback();
                    }

                    // A truncated final packet can leave channels ragged; trim
                    // to the shortest so `frames` is valid for every channel.
                    let frames = planes.iter().map(|p| p.len()).min().unwrap_or(0);
                    let channels = planes.len();
                    let mut samples = Vec::with_capacity(frames * channels);
                    for plane in &planes {
                        samples.extend_from_slice(&plane[..frames]);
                    }
                    return DecodedAudio { samples: Arc::new(samples), frames, channels, sample_rate: source_rate };
                }
                eprintln!("decode_audio_file: Unsupported codec for: {}", path);
            } else {
                eprintln!("decode_audio_file: No default track for: {}", path);
            }
        } else {
            eprintln!("decode_audio_file: Unsupported format or corrupt audio: {}", path);
        }
    } else {
        eprintln!("decode_audio_file: Failed to open file: {}", path);
    }
    DecodedAudio::silent_fallback()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_folder_monitor_non_existent_path_safety() {
        let sample_registry = Arc::new(nullherz_dna::SampleRegistry::new());

        let db_path = "test_folder_monitor.redb";
        let _ = std::fs::remove_file(db_path);
        let library_db = LibraryDatabase::load(db_path).unwrap();
        let library = Arc::new(parking_lot::Mutex::new(library_db));

        let monitor = FolderMonitor::new(sample_registry, library);

        // Scanning a non-existent folder must return immediately and never panic
        monitor.scan_folder("/non_existent_directory_safely_ignored_by_rayon");

        // Loading a corrupt or non-existent file path must fallback gracefully to silent buffer
        monitor.load_and_register("/non_existent_audio_file.wav");

        let _ = std::fs::remove_file(db_path);
    }

    /// The periodic rescan must NOT re-decode a file already in the registry.
    /// Re-decoding every file every cycle (the old order: decode before the
    /// existence check) doubled memory on a loop and froze the app on large
    /// libraries. A second scan of the same file must reuse the existing
    /// buffer Arc rather than decoding and registering a fresh one.
    #[test]
    fn test_rescan_skips_already_registered_file() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let dir = std::env::temp_dir().join(format!("nh_rescan_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let wav = dir.join("tone.wav");
        let wav_str = wav.to_str().unwrap().to_string();
        {
            let spec = hound::WavSpec {
                channels: 1, sample_rate: 44100, bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut w = hound::WavWriter::create(&wav, spec).unwrap();
            for i in 0..4410 {
                w.write_sample(((i as f32 * 0.1).sin() * 10000.0) as i16).unwrap();
            }
            w.finalize().unwrap();
        }

        let registry = Arc::new(nullherz_dna::SampleRegistry::new());
        let db_path = dir.join("lib.redb");
        let library = Arc::new(parking_lot::Mutex::new(
            LibraryDatabase::load(db_path.to_str().unwrap()).unwrap(),
        ));
        let monitor = FolderMonitor::new(registry.clone(), library);

        let mut hasher = DefaultHasher::new();
        wav_str.hash(&mut hasher);
        let id = hasher.finish();

        // First scan decodes and registers.
        monitor.load_and_register(&wav_str);
        let first = registry.get(id).expect("registered on first scan").buffer;

        // Second scan must SKIP — same buffer Arc, proving no re-decode.
        monitor.load_and_register(&wav_str);
        let second = registry.get(id).expect("still registered").buffer;

        assert!(
            Arc::ptr_eq(&first, &second),
            "rescan re-decoded an already-registered file — the freeze regression is back"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod stereo_decode_tests {
    use super::*;

    /// Write a stereo WAV whose two channels carry DIFFERENT tones, so any
    /// channel confusion is visible rather than averaged away.
    fn write_stereo_wav(path: &str, left_hz: f32, right_hz: f32, seconds: f32) -> usize {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let frames = (44100.0 * seconds) as usize;
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..frames {
            let t = i as f32 / 44100.0;
            let l = (2.0 * std::f32::consts::PI * left_hz * t).sin() * 0.5;
            let r = (2.0 * std::f32::consts::PI * right_hz * t).sin() * 0.5;
            w.write_sample((l * 32767.0) as i16).unwrap();
            w.write_sample((r * 32767.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        frames
    }

    fn dominant_hz(samples: &[f32]) -> f32 {
        let crossings = samples.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        crossings as f32 * 44100.0 / samples.len() as f32
    }

    /// A decoded stereo file must expose its frame count and channel layout
    /// truthfully. Interleaving channels into one flat buffer and calling it a
    /// mono stream is what made every stereo track play at 2x speed: the voice
    /// advances one ELEMENT per output frame, so L,R,L,R is consumed as four
    /// consecutive mono frames.
    #[test]
    fn test_stereo_decode_is_planar_with_channel_count() {
        let path = std::env::temp_dir().join("nullherz_stereo_decode.wav");
        let path = path.to_string_lossy().to_string();
        let frames = write_stereo_wav(&path, 440.0, 880.0, 1.0);

        let decoded = decode_audio_file(&path);

        assert_eq!(
            decoded.channels, 2,
            "a 2-channel file must report 2 channels"
        );
        assert_eq!(
            decoded.frames, frames,
            "frame count must be per-channel frames, not total samples"
        );
        assert_eq!(decoded.samples.len(), frames * 2, "planar buffer holds every sample");

        // Planar layout: channel 0 occupies the first `frames` slots, channel 1
        // the next. Each must carry its OWN tone.
        let left = &decoded.samples[..frames];
        let right = &decoded.samples[frames..];
        let left_hz = dominant_hz(left);
        let right_hz = dominant_hz(right);

        assert!(
            (left_hz - 440.0).abs() < 5.0,
            "left channel should be 440 Hz, measured {:.1} Hz",
            left_hz
        );
        assert!(
            (right_hz - 880.0).abs() < 10.0,
            "right channel should be 880 Hz, measured {:.1} Hz",
            right_hz
        );

        let _ = std::fs::remove_file(&path);
    }

    /// A mono file must still decode to exactly one channel, unchanged.
    #[test]
    fn test_mono_decode_is_unchanged() {
        let path = std::env::temp_dir().join("nullherz_mono_decode.wav");
        let path = path.to_string_lossy().to_string();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let frames = 44100usize;
        {
            let mut w = hound::WavWriter::create(&path, spec).unwrap();
            for i in 0..frames {
                let v = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin() * 0.5;
                w.write_sample((v * 32767.0) as i16).unwrap();
            }
            w.finalize().unwrap();
        }

        let decoded = decode_audio_file(&path);

        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.frames, frames);
        assert_eq!(decoded.samples.len(), frames);
        let hz = dominant_hz(&decoded.samples);
        assert!((hz - 440.0).abs() < 5.0, "mono 440 Hz, measured {:.1}", hz);

        let _ = std::fs::remove_file(&path);
    }
}
