use std::sync::Arc;
use nullherz_dna::{SampleRegistry, LibraryDatabase, LibraryTrack, SampleMetadata};
use std::path::Path;
use std::time::Duration;

pub struct FolderMonitor {
    sample_registry: Arc<SampleRegistry>,
    library: Arc<std::sync::Mutex<LibraryDatabase>>,
}

impl FolderMonitor {
    pub fn new(sample_registry: Arc<SampleRegistry>, library: Arc<std::sync::Mutex<LibraryDatabase>>) -> Self {
        Self {
            sample_registry,
            library,
        }
    }

    pub fn scan_folder(&self, path: &str) {
        let path = Path::new(path);
        if !path.is_dir() { return; }

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let file_path = entry.path();
                if file_path.is_file() {
                    if let Some(ext) = file_path.extension() {
                        if ext == "wav" {
                            self.load_and_register(file_path.to_str().unwrap());
                        }
                    }
                }
            }
        }
    }

    fn load_and_register(&self, path: &str) {
        // High-Quality WAV Loader for Alpha
        let buffer = if let Ok(mut reader) = hound::WavReader::open(path) {
            let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect();
            Arc::new(samples)
        } else {
            Arc::new(vec![0.0f32; 44100 * 5]) // Fallback to silent buffer
        };

        let id = path.len() as u64; // Simple ID for demo

        let mut lib = self.library.lock().unwrap();
        if lib.tracks.iter().any(|t| t.path == path) { return; }

        let track = LibraryTrack {
            id,
            path: path.to_string(),
            title: Path::new(path).file_name().unwrap().to_str().unwrap().to_string(),
            artist: "Unknown".to_string(),
            metadata: SampleMetadata::new_empty(),
        };

        lib.tracks.push(track);
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
