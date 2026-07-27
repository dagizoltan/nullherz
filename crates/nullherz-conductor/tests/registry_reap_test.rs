//! `tick()` must bound how much decoded audio a session holds.
//!
//! The library scanner registers the full decoded audio of every file it finds
//! so the analysis worker can read it, and nothing released it afterwards: a
//! 500-track folder meant every track resident for the whole session. The reap
//! is what makes residency a function of what is IN USE rather than of how big
//! the user's library is.
//!
//! The dangerous half is what must NOT be evicted. The registry also holds
//! samples that exist nowhere else — transfusion children, captures, chopped
//! edits. Those have no file to decode from, so evicting one silently destroys
//! the user's work. A reap that only checked "is anything using this?" would do
//! exactly that.

use nullherz_conductor::orchestrator::Conductor;
use std::sync::Arc;

/// The reaper is opt-in (see `Conductor::reap_registry` for why it is not on by
/// default). These tests exercise the policy, so they turn it on. Serialised
/// because the switch is a process-wide env var.
fn with_reap_enabled(f: impl FnOnce()) {
    // parking_lot, per the workspace lint: it has no poisoning, so a panicking
    // test cannot leave the gate permanently unusable for the others.
    static GATE: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    let _g = GATE.lock();
    unsafe { std::env::set_var("NULLHERZ_REGISTRY_REAP", "1") };
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    unsafe { std::env::remove_var("NULLHERZ_REGISTRY_REAP") };
    if let Err(e) = r { std::panic::resume_unwind(e); }
}

/// Write a real WAV so the "recoverable" check has a file to find.
fn write_wav(dir: &std::path::Path, name: &str, frames: usize) -> String {
    let path = dir.join(name);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    for i in 0..frames {
        w.write_sample((i as f32 * 0.001).sin() * 0.5).unwrap();
    }
    w.finalize().unwrap();
    path.to_string_lossy().into_owned()
}

fn save_track(conductor: &Conductor, id: u64, path: &str) {
    use nullherz_dna::GeneticLibrary;
    let lib = conductor.library.lock();
    let _ = lib.save_track(&nullherz_dna::LibraryTrack {
        id,
        path: path.to_string(),
        title: format!("t{id}"),
        artist: "a".into(),
        album: "b".into(),
        genre: "c".into(),
        energy_level: 0.5,
        metadata: Arc::new(nullherz_traits::SampleMetadata::new_empty()),
    });
}

#[test]
fn test_reap_releases_scanned_tracks_but_keeps_what_is_on_a_deck() { with_reap_enabled(|| {
    let dir = std::env::temp_dir().join(format!("nullherz_reap_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let mut conductor = Conductor::with_library_path(":memory:");
    let registry = conductor.transfusion_manager.sample_registry.clone();

    const N: u64 = 12;
    let mut weaks = Vec::new();
    for id in 0..N {
        let path = write_wav(&dir, &format!("t{id}.wav"), 1_024);
        save_track(&conductor, id, &path);
        let buf: Arc<Vec<f32>> = Arc::new(vec![0.1f32; 1_024]);
        weaks.push(Arc::downgrade(&buf));
        registry.register(id, buf);
    }
    assert_eq!(registry.list_ids().len(), N as usize, "precondition: all registered");

    // Deck A holds track 0 — the one thing the reap must leave alone.
    conductor.mixer_manager.deck_samples.insert('A', 0);

    conductor.tick();

    let ids = registry.list_ids();
    assert!(
        ids.contains(&0),
        "the sample loaded on deck A was evicted; sync_sampler_metadata reads that entry"
    );
    assert_eq!(
        ids.len(),
        1,
        "expected only the deck-held sample to remain, got {ids:?} — residency still \
         scales with library size rather than with what is in use"
    );

    registry.drain_garbage();
    let resident = weaks.iter().filter(|w| w.upgrade().is_some()).count();
    assert_eq!(
        resident, 1,
        "{resident} decoded buffers are still in memory after the reap; eviction \
         unlisted them without freeing them"
    );

    let _ = std::fs::remove_dir_all(&dir);
}); }

#[test]
fn test_reap_never_evicts_a_sample_that_cannot_be_recovered() { with_reap_enabled(|| {
    // A transfusion child / capture / chop lives only in the registry. There is
    // no file to decode it back from, so evicting it is data loss, not caching.
    let mut conductor = Conductor::with_library_path(":memory:");
    let registry = conductor.transfusion_manager.sample_registry.clone();

    let orphan: Arc<Vec<f32>> = Arc::new(vec![0.7f32; 512]);
    let weak = Arc::downgrade(&orphan);
    registry.register(9_001, orphan); // deliberately NOT saved to the library

    conductor.tick();
    registry.drain_garbage();

    assert!(
        registry.get(9_001).is_some(),
        "a sample with no file on disk was evicted — that audio existed only here, so \
         the reap destroyed it"
    );
    assert!(weak.upgrade().is_some(), "the orphan sample's audio was freed");
}); }

#[test]
fn test_reap_keeps_a_track_whose_file_has_disappeared() { with_reap_enabled(|| {
    // Recoverability is about the file being there NOW. A track whose file was
    // moved or unplugged (external drive) cannot be re-decoded, so its resident
    // copy is the only one left.
    let dir = std::env::temp_dir().join(format!("nullherz_reap_gone_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = write_wav(&dir, "vanishing.wav", 256);

    let mut conductor = Conductor::with_library_path(":memory:");
    let registry = conductor.transfusion_manager.sample_registry.clone();
    save_track(&conductor, 42, &path);
    registry.register(42, Arc::new(vec![0.3f32; 256]));

    std::fs::remove_file(&path).unwrap(); // drive unplugged / file moved

    conductor.tick();

    assert!(
        registry.get(42).is_some(),
        "evicted a track whose file no longer exists — the deck-load path would find \
         neither the buffer nor the file, so the track becomes unplayable"
    );

    let _ = std::fs::remove_dir_all(&dir);
}); }

#[test]
fn test_reap_leaves_in_flight_hydrations_alone() { with_reap_enabled(|| {
    // A decode running right now will register on completion. Evicting the id
    // mid-flight races that, and the reap must not fight the loader.
    let dir = std::env::temp_dir().join(format!("nullherz_reap_hyd_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = write_wav(&dir, "loading.wav", 256);

    let mut conductor = Conductor::with_library_path(":memory:");
    let registry = conductor.transfusion_manager.sample_registry.clone();
    save_track(&conductor, 77, &path);
    registry.register(77, Arc::new(vec![0.9f32; 256]));
    conductor.hydration_pending.insert(77);

    conductor.tick();

    assert!(
        registry.get(77).is_some(),
        "reaped a sample that is mid-hydration"
    );

    let _ = std::fs::remove_dir_all(&dir);
}); }
