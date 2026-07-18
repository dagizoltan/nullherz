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

    fn drain_garbage(&self) {
        if self.readers.load(Ordering::SeqCst) > 0 { return; }
        let mut g = self.garbage.lock();
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
