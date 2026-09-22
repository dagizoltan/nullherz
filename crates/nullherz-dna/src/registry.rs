use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicPtr, Ordering};
use crate::*;

/// High-performance Sample Registry using a multi-tiered lock-free approach.
/// Stage 2: Optimized for O(1) concurrent registration and high-speed lookups.
pub struct SampleRegistry {
    inner: AtomicPtr<HashMap<u64, RegisteredSample>>,
    write_lock: Mutex<()>,
    garbage: Mutex<Vec<*mut HashMap<u64, RegisteredSample>>>,
    readers: std::sync::atomic::AtomicUsize,
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
        let _lock = self.write_lock.lock();

        let old_ptr = self.inner.load(Ordering::Acquire);
        let mut new_map = unsafe { (*old_ptr).clone() };
        new_map.insert(id, RegisteredSample { buffer, metadata });

        let new_ptr = Box::into_raw(Box::new(new_map));
        self.inner.store(new_ptr, Ordering::Release);
        self.garbage.lock().push(old_ptr);
    }

    fn remove(&self, id: u64) -> Option<RegisteredSample> {
        let _lock = self.write_lock.lock();

        let old_ptr = self.inner.load(Ordering::Acquire);
        // Check before cloning: a miss is the common case when a caller sweeps
        // ids speculatively, and cloning the whole map to discover that would
        // make eviction cost more than the leak.
        if unsafe { !(*old_ptr).contains_key(&id) } {
            return None;
        }

        let mut new_map = unsafe { (*old_ptr).clone() };
        let evicted = new_map.remove(&id);

        let new_ptr = Box::into_raw(Box::new(new_map));
        self.inner.store(new_ptr, Ordering::Release);
        // The retired map is reclaimed by drain_garbage once no reader holds
        // it. Note this frees the MAP, not the sample: the sample's buffer is
        // owned by the `Arc` we hand back, and dies with the caller's copy.
        self.garbage.lock().push(old_ptr);

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
        let mut g = self.garbage.lock();
        if self.readers.load(Ordering::SeqCst) > 0 { return; }
        for ptr in g.drain(..) {
            unsafe { drop(Box::from_raw(ptr)); }
        }
    }

    fn get(&self, id: u64) -> Option<RegisteredSample> {
        self.readers.fetch_add(1, Ordering::SeqCst);
        let ptr = self.inner.load(Ordering::Acquire);
        let res = unsafe { (*ptr).get(&id).cloned() };
        self.readers.fetch_sub(1, Ordering::SeqCst);
        res
    }

    fn list_ids(&self) -> Vec<u64> {
        self.readers.fetch_add(1, Ordering::SeqCst);
        let ptr = self.inner.load(Ordering::Acquire);
        let res = unsafe { (*ptr).keys().cloned().collect() };
        self.readers.fetch_sub(1, Ordering::SeqCst);
        res
    }
}

impl SampleRegistry {
    pub fn new() -> Self {
        let initial_map = Box::new(HashMap::new());
        Self {
            inner: AtomicPtr::new(Box::into_raw(initial_map)),
            write_lock: Mutex::new(()),
            garbage: Mutex::new(Vec::new()),
            readers: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl Drop for SampleRegistry {
    fn drop(&mut self) {
        use nullherz_traits::SampleRegistry;
        let ptr = self.inner.load(Ordering::Acquire);
        unsafe { drop(Box::from_raw(ptr)); }
        self.drain_garbage();
    }
}
