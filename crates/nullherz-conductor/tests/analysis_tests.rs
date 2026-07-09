use nullherz_traits::SampleRegistry;
use nullherz_dna::{ LibraryDatabase, LibraryTrack, GeneticLibrary};
use nullherz_traits::SampleMetadata;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_analysis_pipeline() {
    let db_path = "test_analysis.redb";
    let _ = std::fs::remove_file(db_path);

    let sample_registry = Arc::new(nullherz_dna::SampleRegistry::new());
    let library = Arc::new(std::sync::Mutex::new(LibraryDatabase::load(db_path).unwrap()));

    // Register a mock sample (1 second of a 4Hz pulse to simulate transients)
    let sample_rate = 44100;
    let mut buffer = vec![0.0f32; sample_rate];
    for i in 0..4 {
        let pos = i * sample_rate / 4;
        buffer[pos] = 1.0; // Sharp transient
    }
    let sample_id = 12345;
    sample_registry.register(sample_id, Arc::new(buffer));

    // Register track in library
    {
        let lib = library.lock().unwrap();
        lib.save_track(&LibraryTrack {
            id: sample_id,
            path: "mock.wav".to_string(),
            title: "Mock Track".to_string(),
            artist: "Mock Artist".to_string(),
            album: "Mock Album".to_string(),
            genre: "Mock Genre".to_string(),
            energy_level: 0.5,
            metadata: Arc::new(SampleMetadata::new_empty()),
        }).unwrap();
    }

    let worker = nullherz_conductor::analysis_worker::AnalysisWorker::new(sample_registry.clone())
        .with_library(library.clone());

    // Run analysis manually (one iteration)
    // We need to use a small hack to call run_once if it's private,
    // or just use the public start and wait.
    // Let's just call start and sleep briefly.
    worker.start();

    // Wait for analysis
    let mut enriched = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(sample) = sample_registry.get(sample_id) {
            if sample.metadata.bpm > 0.0 {
                enriched = true;
                break;
            }
        }
    }

    assert!(enriched, "Sample metadata should be enriched with BPM");

    let sample = sample_registry.get(sample_id).unwrap();
    println!("Detected BPM: {}", sample.metadata.bpm);
    assert!(sample.metadata.bpm > 0.0);
    assert!(!sample.metadata.transients.is_empty());
    assert!(!sample.metadata.peaks.is_empty());

    // Verify database sync
    {
        let lib = library.lock().unwrap();
        let track = lib.get_track(sample_id).unwrap().unwrap();
        assert!(track.metadata.bpm > 0.0);
    }

    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn test_root_key_detection() {
    let sample_registry = Arc::new(nullherz_dna::SampleRegistry::new());

    // Generate a 440Hz Sine Wave (Note A)
    let sample_rate = 44100;
    let freq = 440.0;
    let mut buffer = vec![0.0f32; sample_rate * 2]; // 2 seconds
    for i in 0..buffer.len() {
        buffer[i] = (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate as f32).sin();
    }

    let sample_id = 999;
    sample_registry.register(sample_id, Arc::new(buffer));

    let worker = nullherz_conductor::analysis_worker::AnalysisWorker::new(sample_registry.clone());

    // Use the internal detect_root_key via a registry update and wait for background thread if we used start(),
    // but for unit test we can just call the logic via a hack or by making it public.
    // Given the current structure, we'll wait for the background analysis.
    worker.start();

    let mut detected_key = None;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(sample) = sample_registry.get(sample_id) {
            if let Some(key) = sample.metadata.root_key {
                detected_key = Some(key);
                break;
            }
        }
    }

    assert!(detected_key.is_some(), "Root key should be detected");
    // 440Hz is A. Pitch class for A is 9 (0=C, 1=C#, 2=D, 3=D#, 4=E, 5=F, 6=F#, 7=G, 8=G#, 9=A, 10=A#, 11=B)
    assert_eq!(detected_key.unwrap() as i32, 9);
}
