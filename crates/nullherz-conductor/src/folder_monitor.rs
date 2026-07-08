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
            if file_path.is_file() {
                if let Some(ext) = file_path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if ext == "wav" || ext == "flac" || ext == "mp3" || ext == "ogg" {
                        self.load_and_register(file_path.to_str().unwrap());
                    }
                }
            }
        }
    }

    fn load_and_register(&self, path: &str) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use symphonia::core::audio::Signal;

        // Use symphonia for multi-format support
        let buffer = if let Ok(file) = std::fs::File::open(path) {
            let mss = symphonia::core::io::MediaSourceStream::new(Box::new(file), Default::default());
            let hint = symphonia::core::probe::Hint::new();
            let mut probed = symphonia::default::get_probe().format(&hint, mss, &Default::default(), &Default::default()).expect("unsupported format");
            let mut decoder = symphonia::default::get_codecs().make(&probed.format.default_track().unwrap().codec_params, &Default::default()).expect("unsupported codec");

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
            metadata,
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
