//! A reclaimed sample must not send the folder scanner back to disk.
//!
//! Regression test for a real one. The scan deduped on registry residency
//! ("already registered? skip"), which was correct only while residency was
//! permanent. Once `Conductor::reap_registry` could release a track, the
//! 10-second sweep saw a registry miss, re-decoded the whole file, and the next
//! reap released it again — an endless decode/evict treadmill on the biggest
//! files in the library. Observed live: the same 121 MB track released twice in
//! one minute, alongside an audio underrun.
//!
//! The two questions the old check conflated: "is this audio resident" and
//! "have we already ingested this file". They are not the same question, and
//! only the second one should gate decoding.

use nullherz_conductor::folder_monitor::FolderMonitor;
use nullherz_traits::SampleRegistry as _;
use std::sync::Arc;

fn write_wav(path: &std::path::Path, frames: usize) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..frames {
        w.write_sample((i as f32 * 0.01).sin() * 0.4).unwrap();
    }
    w.finalize().unwrap();
}

/// Registry wrapper that counts registrations, so "did the scanner decode this
/// again?" becomes observable.
struct CountingRegistry {
    inner: Arc<nullherz_dna::SampleRegistry>,
    registrations: std::sync::atomic::AtomicUsize,
}

impl nullherz_traits::SampleRegistry for CountingRegistry {
    fn get(&self, id: u64) -> Option<nullherz_traits::RegisteredSample> { self.inner.get(id) }
    fn register(&self, id: u64, buffer: Arc<Vec<f32>>) {
        self.registrations.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.register(id, buffer);
    }
    fn register_with_metadata(
        &self,
        id: u64,
        buffer: Arc<Vec<f32>>,
        metadata: Arc<nullherz_traits::SampleMetadata>,
    ) {
        self.registrations.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.register_with_metadata(id, buffer, metadata);
    }
    fn drain_garbage(&self) { self.inner.drain_garbage() }
    fn list_ids(&self) -> Vec<u64> { self.inner.list_ids() }
    fn remove(&self, id: u64) -> Option<nullherz_traits::RegisteredSample> { self.inner.remove(id) }
}

#[test]
fn test_scan_does_not_re_decode_a_reaped_track() {
    let dir = std::env::temp_dir().join(format!("nullherz_thrash_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    write_wav(&dir.join("a.wav"), 4_096);
    write_wav(&dir.join("b.wav"), 4_096);

    let counting = Arc::new(CountingRegistry {
        inner: Arc::new(nullherz_dna::SampleRegistry::new()),
        registrations: std::sync::atomic::AtomicUsize::new(0),
    });
    let library = Arc::new(parking_lot::Mutex::new(
        nullherz_dna::LibraryDatabase::load(":memory:").unwrap(),
    ));
    let monitor = FolderMonitor::new(counting.clone(), library);

    let dir_str = dir.to_str().unwrap();
    monitor.scan_folder_sync(dir_str);
    let after_first = counting.registrations.load(std::sync::atomic::Ordering::SeqCst);
    assert!(after_first >= 2, "first scan should ingest both files, got {after_first}");

    // A second scan with everything still resident must be a no-op.
    monitor.scan_folder_sync(dir_str);
    assert_eq!(
        counting.registrations.load(std::sync::atomic::Ordering::SeqCst),
        after_first,
        "a scan over already-ingested, still-resident files re-registered them"
    );

    // Now simulate the reaper: release the audio, exactly as reap_registry does.
    for id in counting.list_ids() {
        drop(counting.remove(id));
    }
    counting.drain_garbage();
    assert!(counting.list_ids().is_empty(), "precondition: everything reaped");

    // THE REGRESSION: the sweep after a reap must not go back to disk.
    monitor.scan_folder_sync(dir_str);
    let after_reap = counting.registrations.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        after_reap, after_first,
        "the scan re-decoded {} file(s) that the reaper had released — this is the \
         decode/evict treadmill: every sweep re-reads the largest files in the library \
         and the next reap throws them away again",
        after_reap - after_first
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_a_genuinely_new_file_is_still_picked_up_after_a_reap() {
    // The dedup must not become a blanket "never scan again" — a file added to
    // the folder later still has to be ingested.
    let dir = std::env::temp_dir().join(format!("nullherz_thrash_new_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    write_wav(&dir.join("first.wav"), 2_048);

    let counting = Arc::new(CountingRegistry {
        inner: Arc::new(nullherz_dna::SampleRegistry::new()),
        registrations: std::sync::atomic::AtomicUsize::new(0),
    });
    let library = Arc::new(parking_lot::Mutex::new(
        nullherz_dna::LibraryDatabase::load(":memory:").unwrap(),
    ));
    let monitor = FolderMonitor::new(counting.clone(), library);
    let dir_str = dir.to_str().unwrap();

    monitor.scan_folder_sync(dir_str);
    for id in counting.list_ids() {
        drop(counting.remove(id));
    }
    counting.drain_garbage();
    let baseline = counting.registrations.load(std::sync::atomic::Ordering::SeqCst);

    write_wav(&dir.join("second.wav"), 2_048);
    monitor.scan_folder_sync(dir_str);

    assert!(
        counting.registrations.load(std::sync::atomic::Ordering::SeqCst) > baseline,
        "a newly added file was ignored — the dedup is now suppressing real work"
    );
    assert_eq!(
        counting.list_ids().len(),
        1,
        "only the new file should be resident; the reaped one must stay released"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
