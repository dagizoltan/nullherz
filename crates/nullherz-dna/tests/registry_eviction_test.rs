//! The registry must be able to let go.
//!
//! It had no removal method at all, so every track a library scan decoded
//! stayed resident for the life of the session — tens of gigabytes for a
//! 500-track folder. These tests pin the two properties that fix requires:
//! eviction actually releases the memory, and it never releases memory that
//! something else is still using.

use nullherz_traits::SampleRegistry as _;
use std::sync::Arc;

fn sample_of(frames: usize) -> Arc<Vec<f32>> {
    Arc::new(vec![0.25f32; frames])
}

#[test]
fn test_evicting_the_only_holder_frees_the_buffer() {
    let reg = nullherz_dna::SampleRegistry::new();
    let buf = sample_of(16_384);
    let weak = Arc::downgrade(&buf);

    reg.register(7, buf);
    // Drop our own handle so the registry is the sole owner. `weak` now tracks
    // exactly whether the registry is holding the audio.
    assert!(weak.upgrade().is_some(), "registry should be holding the buffer");

    let evicted = reg.remove(7);
    assert!(evicted.is_some(), "remove() must report what it evicted");
    drop(evicted);

    // Eviction alone does NOT free. This is an RCU registry: `remove` publishes
    // a new map and retires the old one, and the retired map still holds an
    // `Arc` to the sample. Memory comes back only when the retired map is
    // reclaimed. Any caller that evicts to relieve memory pressure and never
    // drains has done nothing at all — which is why `Conductor::reap_registry`
    // drains after a sweep.
    assert!(
        weak.upgrade().is_some(),
        "expected the retired map to still pin the buffer until drain_garbage()"
    );
    assert!(reg.get(7).is_none(), "evicted id must no longer resolve, even before draining");

    reg.drain_garbage();

    assert!(
        weak.upgrade().is_none(),
        "the buffer is still alive after eviction AND drain — the registry leaked a \
         reference, so bounded residency is not actually bounded"
    );
}

#[test]
fn test_eviction_does_not_pull_the_buffer_from_a_live_user() {
    // A sampler holds its own Arc to the audio it is playing. Eviction must be
    // a refcount decrement, never a free, or evicting a loaded deck would yank
    // the audio out from under the audio thread mid-block.
    let reg = nullherz_dna::SampleRegistry::new();
    let buf = sample_of(4_096);
    reg.register(9, buf);

    let in_use = reg.get(9).expect("registered").buffer;
    let weak = Arc::downgrade(&in_use);

    drop(reg.remove(9));

    assert!(
        weak.upgrade().is_some(),
        "eviction freed a buffer that was still referenced — this is the use-after-free \
         a naive `remove` would cause on the audio thread"
    );
    assert_eq!(in_use[0], 0.25, "the held buffer must still be readable and intact");
    assert!(reg.get(9).is_none(), "the registry entry is gone even though the audio lives on");
}

#[test]
fn test_removing_an_unknown_id_is_a_no_op() {
    let reg = nullherz_dna::SampleRegistry::new();
    reg.register(1, sample_of(64));
    assert!(reg.remove(999).is_none(), "unknown id must report None");
    assert!(reg.get(1).is_some(), "an unrelated entry must survive a missed eviction");
}

#[test]
fn test_residency_falls_when_a_scan_is_reaped() {
    // The shape of the actual bug: register a folder's worth of tracks, then
    // release the ones nothing is using, and confirm the audio is genuinely
    // gone rather than merely unlisted.
    let reg = nullherz_dna::SampleRegistry::new();
    const N: u64 = 64;
    const FRAMES: usize = 8_192;

    let mut weaks = Vec::new();
    for id in 0..N {
        let buf = sample_of(FRAMES);
        weaks.push(Arc::downgrade(&buf));
        reg.register(id, buf);
    }
    assert_eq!(reg.list_ids().len(), N as usize);

    // Two "decks" keep hold of their audio; everything else is reapable.
    let pinned_a = reg.get(0).unwrap().buffer;
    let pinned_b = reg.get(1).unwrap().buffer;

    for id in 2..N {
        drop(reg.remove(id));
    }
    reg.drain_garbage();

    assert_eq!(reg.list_ids().len(), 2, "only the pinned samples should remain listed");

    let still_alive = weaks.iter().filter(|w| w.upgrade().is_some()).count();
    assert_eq!(
        still_alive, 2,
        "{still_alive} buffers are still resident after reaping {} of {N} — residency \
         did not actually fall",
        N - 2
    );
    // And the pinned ones are untouched.
    assert_eq!(pinned_a.len(), FRAMES);
    assert_eq!(pinned_b.len(), FRAMES);
}

#[test]
fn test_register_after_evict_restores_the_sample() {
    // Eviction is only safe because the sample can be brought back — the deck
    // load path re-decodes on a registry miss. If re-registration did not
    // restore a fully usable entry, reaping would break playback.
    let reg = nullherz_dna::SampleRegistry::new();
    let meta = Arc::new(nullherz_traits::SampleMetadata::new_empty());
    reg.register_with_metadata(3, sample_of(2_048), meta.clone());
    drop(reg.remove(3));
    assert!(reg.get(3).is_none());

    reg.register_with_metadata(3, sample_of(2_048), meta);
    let back = reg.get(3).expect("re-registration must restore the entry");
    assert_eq!(back.buffer.len(), 2_048);
}

#[test]
fn test_concurrent_readers_survive_eviction() {
    // The registry is RCU: readers hold a raw pointer to the map while writers
    // swap in a new one. Eviction is a write, so it must not free a map a
    // reader is walking.
    use std::sync::atomic::{AtomicBool, Ordering};
    let reg = Arc::new(nullherz_dna::SampleRegistry::new());
    for id in 0..128u64 {
        reg.register(id, sample_of(256));
    }

    let stop = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let reg = reg.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut seen = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    for id in 0..128u64 {
                        if let Some(s) = reg.get(id) {
                            // Touch the audio so a freed buffer would fault.
                            seen = seen.wrapping_add(s.buffer[0].to_bits() as u64);
                        }
                    }
                }
                seen
            })
        })
        .collect();

    for id in 0..128u64 {
        drop(reg.remove(id));
        reg.drain_garbage();
    }
    stop.store(true, Ordering::Relaxed);
    for r in readers {
        r.join().expect("a reader thread crashed while the registry was being reaped");
    }
    assert!(reg.list_ids().is_empty());
}
