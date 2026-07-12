use nullherz_traits::SampleRegistry;
use std::sync::Arc;
use nullherz_dna::{ LibraryDatabase, LibraryTrack, GeneticLibrary};
use nullherz_traits::SampleMetadata;
use std::path::Path;
use std::time::Duration;

pub struct FolderMonitor {
    sample_registry: Arc<dyn SampleRegistry>,
    library: Arc<std::sync::Mutex<LibraryDatabase>>,
}

impl FolderMonitor {
    pub fn new(sample_registry: Arc<dyn SampleRegistry>, library: Arc<std::sync::Mutex<LibraryDatabase>>) -> Self {
        Self {
            sample_registry,
            library,
        }
    }

    pub fn scan_folder(&self, path: &str) {
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
        use symphonia::core::audio::Signal;

        // Use symphonia for multi-format support with panic-safe fallback bounds checks
        let buffer = if let Ok(file) = std::fs::File::open(path) {
            let mss = symphonia::core::io::MediaSourceStream::new(Box::new(file), Default::default());
            let hint = symphonia::core::probe::Hint::new();

            if let Ok(probed_res) = symphonia::default::get_probe().format(&hint, mss, &Default::default(), &Default::default()) {
                let mut probed = probed_res;
                if let Some(track) = probed.format.default_track() {
                    if let Ok(mut decoder) = symphonia::default::get_codecs().make(&track.codec_params, &Default::default()) {
                        let mut samples = Vec::new();
                        let mut sample_buf = None;

                        while let Ok(packet) = probed.format.next_packet() {
                            if let Ok(decoded) = decoder.decode(&packet) {
                                if sample_buf.is_none() {
                                    sample_buf = Some(symphonia::core::audio::AudioBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec()));
                                }
                                let buf = sample_buf.as_mut().unwrap();
                                decoded.convert(buf);

                                // Multi-channel support: Interleave channels for the registry
                                let chan_len = buf.frames();
                                let num_chans = buf.spec().channels.count();
                                for i in 0..chan_len {
                                    for c in 0..num_chans {
                                        samples.push(buf.chan(c)[i]);
                                    }
                                }
                            }
                        }
                        Arc::new(samples)
                    } else {
                        eprintln!("FolderMonitor: Unsupported codec for: {}", path);
                        Arc::new(vec![0.0f32; 44100 * 5])
                    }
                } else {
                    eprintln!("FolderMonitor: No default track for: {}", path);
                    Arc::new(vec![0.0f32; 44100 * 5])
                }
            } else {
                eprintln!("FolderMonitor: Unsupported format or corrupt audio: {}", path);
                Arc::new(vec![0.0f32; 44100 * 5])
            }
        } else {
            eprintln!("FolderMonitor: Failed to open file: {}", path);
            Arc::new(vec![0.0f32; 44100 * 5]) // Fallback to silent buffer
        };

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        let id = hasher.finish();

        let lib = self.library.lock().unwrap();
        if let Ok(Some(_)) = lib.get_track(id) { return; }

        let mut metadata = SampleMetadata::new_empty();
        metadata.total_samples = buffer.len() as u64;

        let track = LibraryTrack {
            id,
            path: path.to_string(),
            title: Path::new(path).file_name().unwrap().to_str().unwrap().to_string(),
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
                self.scan_folder(&path);
                std::thread::sleep(Duration::from_secs(10));
            }
        });
    }
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
        let library = Arc::new(std::sync::Mutex::new(library_db));

        let monitor = FolderMonitor::new(sample_registry, library);

        // Scanning a non-existent folder must return immediately and never panic
        monitor.scan_folder("/non_existent_directory_safely_ignored_by_rayon");

        // Loading a corrupt or non-existent file path must fallback gracefully to silent buffer
        monitor.load_and_register("/non_existent_audio_file.wav");

        let _ = std::fs::remove_file(db_path);
    }
}
