use std::sync::Arc;
use nullherz_dna::{SampleRegistry, LibraryDatabase, LibraryTrack, GeneticLibrary};
use nullherz_traits::SampleMetadata;
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

        for entry in walkdir::WalkDir::new(path).into_iter().flatten() {
            let file_path = entry.path();
            if file_path.is_file()
                && let Some(ext) = file_path.extension()
                    && ext == "wav" {
                        self.load_and_register(file_path.to_str().unwrap());
                    }
        }
    }

    fn load_and_register(&self, path: &str) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // High-Quality WAV Loader for Alpha
        let buffer = if let Ok(mut reader) = hound::WavReader::open(path) {
            let spec = reader.spec();
            match (spec.sample_format, spec.bits_per_sample) {
                (hound::SampleFormat::Float, 32) => {
                    let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect();
                    Arc::new(samples)
                }
                (hound::SampleFormat::Int, 16) => {
                    let samples: Vec<f32> = reader.samples::<i16>().map(|s| s.unwrap_or(0) as f32 / 32768.0).collect();
                    Arc::new(samples)
                }
                _ => {
                    eprintln!("FolderMonitor: Unsupported WAV format ({:?}, {} bits)", spec.sample_format, spec.bits_per_sample);
                    Arc::new(vec![0.0f32; 44100 * 5])
                }
            }
        } else {
            eprintln!("FolderMonitor: Failed to open WAV file: {}", path);
            Arc::new(vec![0.0f32; 44100 * 5]) // Fallback to silent buffer
        };

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        let id = hasher.finish();

        let lib = self.library.lock().unwrap();
        if let Ok(Some(_)) = lib.get_track(id) { return; }

        let track = LibraryTrack {
            id,
            path: path.to_string(),
            title: Path::new(path).file_name().unwrap().to_str().unwrap().to_string(),
            artist: "Unknown".to_string(),
            album: "Unknown".to_string(),
            genre: "Unknown".to_string(),
            energy_level: 0.5,
            metadata: SampleMetadata::new_empty(),
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
