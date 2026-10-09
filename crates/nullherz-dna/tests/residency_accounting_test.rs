//! Residency is accounted, not measured, so it has to stay equal to the truth.
//!
//! `SampleRegistry::residency()` is read from the conductor tick and is the
//! number the registry reaper evicts against, so a drifting counter is not a
//! cosmetic bug: too high and the reaper evicts audio the user still needs, too
//! low and the scanner runs the machine out of memory (which it has done — see
//! `reap_registry`'s note on a 2 GB decode failing a one-minute run).
//!
//! It replaced a walk that totalled the map on every read. A walk cannot drift;
//! an accumulator can, so each mutator is pinned here against a recount.

use nullherz_traits::SampleRegistry as _;
use std::sync::Arc;

fn registry() -> nullherz_dna::SampleRegistry {
    nullherz_dna::SampleRegistry::new()
}

fn buffer(frames: usize) -> nullherz_dna::SampleBuffer {
    Arc::new(vec![0.0f32; frames]).into()
}

fn meta() -> Arc<nullherz_traits::SampleMetadata> {
    Arc::new(nullherz_traits::SampleMetadata::new_empty())
}

/// The truth, computed the way the old code did.
fn recount(reg: &nullherz_dna::SampleRegistry) -> (u32, u64) {
    let ids = reg.list_ids();
    let mut bytes = 0u64;
    for id in &ids {
        if let Some(s) = reg.get(*id) {
            bytes += (s.buffer.len() * std::mem::size_of::<f32>()) as u64;
        }
    }
    (ids.len() as u32, bytes)
}

fn assert_agrees(reg: &nullherz_dna::SampleRegistry, what: &str) {
    let (count, bytes) = recount(reg);
    let r = reg.residency();
    assert_eq!(
        (r.count, r.bytes), (count, bytes),
        "residency disagrees with a recount after {what}: accounted {:?}, actual ({count}, {bytes})",
        (r.count, r.bytes)
    );
}

#[test]
fn test_registering_accounts_count_and_bytes() {
    let reg = registry();
    assert_agrees(&reg, "construction");

    for id in 0..32u64 {
        reg.register_with_metadata(id, buffer(64), meta());
    }
    assert_eq!(reg.residency().count, 32);
    assert_eq!(reg.residency().bytes, 32 * 64 * 4);
    assert_agrees(&reg, "32 registrations");
}

/// The case an accumulator gets wrong: the enrichment paths re-register an id
/// to attach analysis metadata. Counting that as a new sample would inflate
/// residency forever, and the reaper would start evicting against a number that
/// is pure accumulation.
#[test]
fn test_re_registering_the_same_id_adjusts_rather_than_accumulates() {
    let reg = registry();
    reg.register_with_metadata(7, buffer(100), meta());
    let first = reg.residency();
    assert_eq!(first.count, 1);
    assert_eq!(first.bytes, 400);

    // Same id, same size: nothing should move.
    reg.register_with_metadata(7, buffer(100), meta());
    assert_eq!(reg.residency().count, 1, "re-registering counted a second sample");
    assert_eq!(reg.residency().bytes, 400, "re-registering double-counted the bytes");
    assert_agrees(&reg, "re-registering the same id at the same size");

    // Same id, bigger buffer: bytes follow the difference, count does not move.
    reg.register_with_metadata(7, buffer(250), meta());
    assert_eq!(reg.residency().count, 1);
    assert_eq!(reg.residency().bytes, 1000);
    assert_agrees(&reg, "re-registering larger");

    // Same id, smaller buffer: and back down.
    reg.register_with_metadata(7, buffer(10), meta());
    assert_eq!(reg.residency().bytes, 40);
    assert_agrees(&reg, "re-registering smaller");
}

#[test]
fn test_removing_returns_the_bytes() {
    let reg = registry();
    for id in 0..10u64 {
        reg.register_with_metadata(id, buffer(32), meta());
    }
    assert_eq!(reg.residency().bytes, 10 * 32 * 4);

    reg.remove(3);
    reg.remove(4);
    assert_eq!(reg.residency().count, 8);
    assert_eq!(reg.residency().bytes, 8 * 32 * 4);
    assert_agrees(&reg, "two removals");

    // Removing an id that is not there must not move anything: `reap_registry`
    // sweeps ids speculatively, so the miss is the common case.
    reg.remove(3);
    reg.remove(9_999);
    assert_eq!(reg.residency().count, 8, "a missing id was counted as an eviction");
    assert_eq!(reg.residency().bytes, 8 * 32 * 4);
    assert_agrees(&reg, "removing absent ids");
}

#[test]
fn test_accounting_survives_a_mixed_workload() {
    let reg = registry();
    // Interleave the operations the scanner, the enrichment paths and the
    // reaper actually produce, then check the accumulator against the truth.
    for round in 0..6u64 {
        for id in 0..40u64 {
            reg.register_with_metadata(id, buffer(16 + (id as usize % 7) * 8), meta());
        }
        for id in (round % 4..40).step_by(5) {
            reg.remove(id);
        }
        for id in 0..12u64 {
            reg.register_with_metadata(id, buffer(64), meta());
        }
        reg.drain_garbage();
        assert_agrees(&reg, &format!("mixed workload round {round}"));
    }
}

/// Reading residency must not hold the reader count up.
///
/// The walk this replaced called `get()` once per sample, and `drain_garbage`
/// declines to reclaim while any reader is mid-`get` — so measuring residency
/// deferred reclamation of the maps registration had retired. Reading it now
/// touches two atomics and no map, so a drain straight afterwards reclaims.
#[test]
fn test_reading_residency_does_not_block_reclamation() {
    let reg = registry();
    for id in 0..64u64 {
        reg.register_with_metadata(id, buffer(16), meta());
    }
    assert!(reg.garbage_len() > 0, "precondition: registration retires maps");

    let _ = reg.residency();
    reg.drain_garbage();
    assert_eq!(
        reg.garbage_len(), 0,
        "retired maps survived a drain taken right after reading residency; reading it is \
         holding the reader count up, which is what the walk did"
    );
}
