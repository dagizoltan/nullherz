use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicPtr, Ordering};
use crate::*;

/// Shards, as a power of two. 64 is enough to make a library scan linear in
/// practice without making `list_ids` or `drain_garbage` walk a silly number of
/// mostly-empty maps.
const SHARD_BITS: u32 = 6;
const SHARDS: usize = 1 << SHARD_BITS;

/// One copy-on-write map. Reads are lock-free; writers clone this shard only.
struct Shard {
    inner: AtomicPtr<HashMap<u64, RegisteredSample>>,
    write_lock: Mutex<()>,
    garbage: Mutex<Vec<*mut HashMap<u64, RegisteredSample>>>,
}

impl Shard {
    fn new() -> Self {
        Self {
            inner: AtomicPtr::new(Box::into_raw(Box::new(HashMap::new()))),
            write_lock: Mutex::new(()),
            garbage: Mutex::new(Vec::new()),
        }
    }
}

/// Which shard owns an id.
///
/// Fibonacci mixing rather than `id % SHARDS`, because both kinds of id in this
/// system would otherwise be awkward: library ids are `DefaultHasher` output
/// (fine either way) but stem and transfusion ids are derived arithmetically
/// (`track.id + (idx + 1) * 10000`), so their low bits are correlated. Taking
/// the top bits of a multiply spreads both.
#[inline]
fn shard_of(id: u64) -> usize {
    const PHI: u64 = 0x9E37_79B9_7F4A_7C15;
    (id.wrapping_mul(PHI) >> (64 - SHARD_BITS)) as usize
}

/// Sample registry: lock-free reads, copy-on-write writes, sharded.
///
/// The read path is load-bearing on the AUDIO THREAD — the engine resolves
/// `AddSourceFromRegistry` inside its block cycle — which is why this is
/// hand-rolled copy-on-write rather than a lock or a `RwLock<HashMap>`.
/// `bench_registry_scale` measures one `get` at ~30 ns, flat in library size.
///
/// Writes clone the map they touch, so sharding is what keeps a bulk scan from
/// being quadratic. Unsharded, registering into a map of N entries cloned all
/// N, and the header here used to claim "O(1) concurrent registration" while
/// measuring:
///
/// ```text
///     N     per register    scan total      sharded
///   100          1.8 us        0.18 ms      0.03 ms
///  1000         15.3 us          15 ms      0.36 ms
///  4000         61.9 us         248 ms       4.2 ms
/// 16000        233.5 us          3.7 s      62.6 ms
/// ```
///
/// — 160x the tracks for ~20,000x the time. With 64 shards a write clones
/// about N/64 entries instead of N, which is the last column: 60x off the
/// 16k scan, and the gap widens with N. `bench_registry_scale` is the source.
pub struct SampleRegistry {
    shards: [Shard; SHARDS],
    /// Global, deliberately. See `drain_garbage`: one reader anywhere defers
    /// reclamation everywhere, which is the conservative direction and keeps
    /// the quiescence argument identical to the unsharded version.
    readers: std::sync::atomic::AtomicUsize,
    /// Running residency, maintained under the touched shard's `write_lock` by
    /// the two mutators. Global rather than per-shard so a read is one load
    /// rather than 64.
    ///
    /// Accounted rather than measured because the caller reads it from the
    /// conductor tick. Walking the map to total it cost 678 µs at 16k samples
    /// and, worse, held `readers` up for the whole walk — which is exactly when
    /// `drain_garbage` declines to reclaim, so the measurement deferred the
    /// memory it was measuring.
    resident_count: std::sync::atomic::AtomicU32,
    resident_bytes: std::sync::atomic::AtomicU64,
}

/// Decoded bytes a sample's buffer occupies.
fn sample_bytes(sample: &RegisteredSample) -> u64 {
    (sample.buffer.len() * std::mem::size_of::<f32>()) as u64
}

unsafe impl Send for SampleRegistry {}
unsafe impl Sync for SampleRegistry {}

impl Default for SampleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl nullherz_traits::SampleRegistry for SampleRegistry {
    fn register(&self, id: u64, buffer: SampleBuffer) {
        self.register_with_metadata(id, buffer, Arc::new(nullherz_traits::SampleMetadata::new_empty()));
    }

    fn register_with_metadata(&self, id: u64, buffer: SampleBuffer, metadata: Arc<nullherz_traits::SampleMetadata>) {
        let shard = &self.shards[shard_of(id)];
        let _lock = shard.write_lock.lock();

        let old_ptr = shard.inner.load(Ordering::Acquire);
        let mut new_map = unsafe { (*old_ptr).clone() };
        let entry = RegisteredSample { buffer, metadata };
        let added = sample_bytes(&entry);
        // `insert` returns the entry this REPLACES. Re-registering an id (the
        // enrichment paths do, to attach analysis metadata) must adjust by the
        // difference, not add again, or residency drifts upward forever and the
        // reaper starts evicting against a number that is pure accumulation.
        let replaced = new_map.insert(id, entry);

        match replaced {
            Some(prev) => {
                // Count is unchanged; only the byte total moves. Read-modify-
                // write is sound because both mutators hold `write_lock`, and
                // saturating both ways means a mis-accounted entry can never
                // wrap the total into a huge number the reaper would act on.
                let removed = sample_bytes(&prev);
                let cur = self.resident_bytes.load(Ordering::Relaxed);
                self.resident_bytes
                    .store(cur.saturating_sub(removed).saturating_add(added), Ordering::Relaxed);
            }
            None => {
                self.resident_count.fetch_add(1, Ordering::Relaxed);
                self.resident_bytes.fetch_add(added, Ordering::Relaxed);
            }
        }

        let new_ptr = Box::into_raw(Box::new(new_map));
        shard.inner.store(new_ptr, Ordering::Release);
        shard.garbage.lock().push(old_ptr);
    }

    fn remove(&self, id: u64) -> Option<RegisteredSample> {
        let shard = &self.shards[shard_of(id)];
        let _lock = shard.write_lock.lock();

        let old_ptr = shard.inner.load(Ordering::Acquire);
        // Check before cloning: a miss is the common case when a caller sweeps
        // ids speculatively, and cloning the whole map to discover that would
        // make eviction cost more than the leak.
        if unsafe { !(*old_ptr).contains_key(&id) } {
            return None;
        }

        let mut new_map = unsafe { (*old_ptr).clone() };
        let evicted = new_map.remove(&id);

        if let Some(ref gone) = evicted {
            self.resident_count.fetch_sub(1, Ordering::Relaxed);
            self.resident_bytes.fetch_sub(sample_bytes(gone), Ordering::Relaxed);
        }

        let new_ptr = Box::into_raw(Box::new(new_map));
        shard.inner.store(new_ptr, Ordering::Release);
        // The retired map is reclaimed by drain_garbage once no reader holds
        // it. Note this frees the MAP, not the sample: the sample's buffer is
        // owned by the `Arc` we hand back, and dies with the caller's copy.
        shard.garbage.lock().push(old_ptr);

        evicted
    }

    /// Reclaim retired maps once no reader can be holding one.
    ///
    /// # The ordering here is the whole correctness argument
    ///
    /// Take the garbage lock FIRST, then check `readers`. It used to be the
    /// other way round, and that is a use-after-free:
    ///
    /// 1. `drain_garbage` reads `readers == 0` and proceeds.
    /// 2. A reader does `fetch_add` and loads `inner` — call it `ptr_X`.
    /// 3. A writer replaces `inner` and pushes `ptr_X` onto the garbage list.
    /// 4. `drain_garbage` finally takes the lock and frees `ptr_X`, which the
    ///    reader from step 2 is still dereferencing.
    ///
    /// With the lock held first, a writer cannot push into the list while we
    /// are draining it. So anything in the list at the moment we check was
    /// retired BEFORE we took the lock — meaning any reader still holding it
    /// must have incremented `readers` before that too, and we see a non-zero
    /// count and bail. A reader arriving after the check can only observe the
    /// CURRENT map, which is by construction not in the list.
    ///
    /// Coarse by design: a single global reader count means one active reader
    /// defers all reclamation. That is the price of a hand-rolled quiescence
    /// check. Retired maps hold `Arc` clones of every sample, so a registry
    /// under continuous read pressure defers the memory too — if that ever
    /// shows up in residency numbers, the answer is `arc-swap` or
    /// `crossbeam-epoch` rather than a cleverer version of this.
    fn drain_garbage(&self) {
        // Per shard, in the same order: take THAT shard's garbage lock, then
        // check readers. Holding shard S's lock is what stops a writer pushing
        // into shard S's list mid-drain, which is all the argument above needs;
        // `readers` being global only makes it more conservative. A reader
        // arriving between two shards simply defers the rest.
        for shard in &self.shards {
            let mut g = shard.garbage.lock();
            if self.readers.load(Ordering::SeqCst) > 0 { return; }
            for ptr in g.drain(..) {
                unsafe { drop(Box::from_raw(ptr)); }
            }
        }
    }

    fn get(&self, id: u64) -> Option<RegisteredSample> {
        self.readers.fetch_add(1, Ordering::SeqCst);
        let ptr = self.shards[shard_of(id)].inner.load(Ordering::Acquire);
        let res = unsafe { (*ptr).get(&id).cloned() };
        self.readers.fetch_sub(1, Ordering::SeqCst);
        res
    }

    fn residency(&self) -> nullherz_traits::Residency {
        nullherz_traits::Residency {
            count: self.resident_count.load(Ordering::Relaxed),
            bytes: self.resident_bytes.load(Ordering::Relaxed),
        }
    }

    fn list_ids(&self) -> Vec<u64> {
        self.readers.fetch_add(1, Ordering::SeqCst);
        // Sized up front from the accounted count: this is the one caller-
        // visible cost sharding adds (a walk of 64 maps instead of one), and
        // reallocating the output while doing it would be gratuitous.
        let mut res = Vec::with_capacity(self.resident_count.load(Ordering::Relaxed) as usize);
        for shard in &self.shards {
            let ptr = shard.inner.load(Ordering::Acquire);
            res.extend(unsafe { (*ptr).keys().copied() });
        }
        self.readers.fetch_sub(1, Ordering::SeqCst);
        res
    }
}

impl SampleRegistry {
    pub fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| Shard::new()),
            readers: std::sync::atomic::AtomicUsize::new(0),
            resident_count: std::sync::atomic::AtomicU32::new(0),
            resident_bytes: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Retired maps awaiting reclamation.
    ///
    /// Diagnostic, not a decision input: each entry holds an `Arc` clone of
    /// every sample it contained, so a rising count is deferred memory, and
    /// `drain_garbage` bails whenever any reader is mid-`get`. Used by
    /// `bench_registry_scale` to show that deferral under read pressure.
    pub fn garbage_len(&self) -> usize {
        self.shards.iter().map(|s| s.garbage.lock().len()).sum()
    }
}

impl Drop for SampleRegistry {
    fn drop(&mut self) {
        use nullherz_traits::SampleRegistry;
        for shard in &self.shards {
            let ptr = shard.inner.load(Ordering::Acquire);
            unsafe { drop(Box::from_raw(ptr)); }
        }
        self.drain_garbage();
    }
}
