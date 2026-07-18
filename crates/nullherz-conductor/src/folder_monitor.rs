// Non-RT plane (library scan worker thread): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
use nullherz_traits::SampleRegistry;
use std::sync::Arc;
use nullherz_dna::{ LibraryDatabase, LibraryTrack, GeneticLibrary};
use nullherz_traits::SampleMetadata;
use std::path::Path;
use std::time::Duration;

pub struct FolderMonitor {
    sample_registry: Arc<dyn SampleRegistry>,
    library: Arc<parking_lot::Mutex<LibraryDatabase>>,
}

impl Clone for FolderMonitor {
    fn clone(&self) -> Self {
        Self {
            sample_registry: self.sample_registry.clone(),
            library: self.library.clone(),
        }
    }
}

impl FolderMonitor {
    pub fn new(sample_registry: Arc<dyn SampleRegistry>, library: Arc<parking_lot::Mutex<LibraryDatabase>>) -> Self {
        Self {
            sample_registry,
            library,
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
        use rayon::prelude::*;
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

        entries.into_par_iter().for_each(|entry| {
            if let Some(path_str) = entry.path().to_str() {
                self.load_and_register(path_str);
            }
        });
    }

    fn load_and_register(&self, path: &str) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let buffer = decode_audio_file(path);

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        let id = hasher.finish();

        // The SampleRegistry is in-memory and empty on EVERY boot; the library
        // is persistent. A library hit must still hydrate the registry, or every
        // deck load after the first-ever scan silently resolves to no buffer
        // (the "eternal silence on second boot" bug). A content change at the
        // same path (regenerated demo tracks) must fall through and re-analyze,
        // or stale metadata/waveforms live forever.
        let existing = { let lib = self.library.lock(); lib.get_track(id).ok().flatten() };
        if let Some(track) = existing {
            if track.metadata.total_samples == buffer.len() as u64 {
                self.sample_registry.register_with_metadata(id, buffer, track.metadata.clone());
                println!("FolderMonitor: Hydrated registry for {}", path);
                return;
            }
            println!("FolderMonitor: Content changed for {}; re-analyzing.", path);
        }
        let lib = self.library.lock();

        let mut metadata = SampleMetadata::new_empty();
        metadata.total_samples = buffer.len() as u64;

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
        self.sample_registry.register(id, buffer);
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


/// Decode any supported audio file to an interleaved f32 buffer.
/// Non-RT; used by the scanner and by on-demand deck-load hydration.
pub fn decode_audio_file(path: &str) -> Arc<Vec<f32>> {
    use symphonia::core::audio::Signal;
    if let Ok(file) = std::fs::File::open(path) {
        let mss = symphonia::core::io::MediaSourceStream::new(Box::new(file), Default::default());
        let hint = symphonia::core::probe::Hint::new();
        if let Ok(mut probed) = symphonia::default::get_probe().format(&hint, mss, &Default::default(), &Default::default()) {
            if let Some(track) = probed.format.default_track() {
                if let Ok(mut decoder) = symphonia::default::get_codecs().make(&track.codec_params, &Default::default()) {
                    let mut samples = Vec::new();
                    let mut sample_buf = None;
                    while let Ok(packet) = probed.format.next_packet() {
                        if let Ok(decoded) = decoder.decode(&packet) {
                            let buf = sample_buf.get_or_insert_with(|| symphonia::core::audio::AudioBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec()));
                            decoded.convert(buf);
                            let chan_len = buf.frames();
                            let num_chans = buf.spec().channels.count();
                            for i in 0..chan_len {
                                for c in 0..num_chans {
                                    samples.push(buf.chan(c)[i]);
                                }
                            }
                        }
                    }
                    return Arc::new(samples);
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
    Arc::new(vec![0.0f32; 44100 * 5]) // silent fallback
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
}
