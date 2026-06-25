use nullherz_dna::{SampleRegistry, LibraryDatabase, LibraryTrack, SampleMetadata};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_analysis_pipeline() {
    let db_path = "test_analysis.redb";
    let _ = std::fs::remove_file(db_path);

    let sample_registry = Arc::new(SampleRegistry::new());
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
            metadata: SampleMetadata::new_empty(),
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
